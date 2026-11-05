param(
    [switch]$SkipTauriBuild,
    [switch]$SkipGpuSidecarBuild,
    [switch]$NoPack,
    [string]$CertPath = "",
    [string]$CertPassword = "",
    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"

function Write-Step($Message) { Write-Host "`n==> $Message" -ForegroundColor Magenta }
function Write-Info($Message) { Write-Host "[INFO] $Message" -ForegroundColor Cyan }
function Write-Success($Message) { Write-Host "[OK] $Message" -ForegroundColor Green }

function Require-Command($Command) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Command"
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "Store MSIX packaging must run on Windows."
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

Require-Command pnpm
Require-Command node
Require-Command cargo
Require-Command winapp

$TargetTriple = "x86_64-pc-windows-msvc"
$TargetDir = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    Join-Path $RepoRoot "src-tauri\target"
} else {
    $env:CARGO_TARGET_DIR
}
$TargetDir = [System.IO.Path]::GetFullPath($TargetDir)
$MainMap = Join-Path $TargetDir "voicetypr.map"
# Encoded flags preserve existing flags and paths containing spaces. Cargo
# gives CARGO_ENCODED_RUSTFLAGS precedence over RUSTFLAGS.
$OriginalEncodedRustFlags = $env:CARGO_ENCODED_RUSTFLAGS
$BuildRustFlags = if (-not [string]::IsNullOrEmpty($OriginalEncodedRustFlags)) {
    @($OriginalEncodedRustFlags -split [char]0x1f)
} elseif (-not [string]::IsNullOrWhiteSpace($env:RUSTFLAGS)) {
    @($env:RUSTFLAGS -split '\s+' | Where-Object { $_ })
} else {
    @()
}
$BuildRustFlags = @($BuildRustFlags) + @("-C", "target-feature=+crt-static")
$SidecarTargetDir = if ([string]::IsNullOrWhiteSpace($env:WHISPER_VULKAN_TARGET_DIR)) {
    $TargetDir
} else {
    $env:WHISPER_VULKAN_TARGET_DIR
}

if (-not $SkipGpuSidecarBuild) {
    try {
        $env:CARGO_ENCODED_RUSTFLAGS = $BuildRustFlags -join [char]0x1f
        cargo build --manifest-path sidecar/whisper-vulkan/Cargo.toml --release --target-dir $SidecarTargetDir
        if ($LASTEXITCODE -ne 0) { throw "Whisper Vulkan sidecar build failed with exit code $LASTEXITCODE" }
    } finally {
        $env:CARGO_ENCODED_RUSTFLAGS = $OriginalEncodedRustFlags
    }

    $SidecarOutDir = Join-Path $RepoRoot "sidecar\whisper-vulkan\dist"
    New-Item -ItemType Directory -Force -Path $SidecarOutDir | Out-Null
    $BuiltSidecar = Join-Path $SidecarTargetDir "release\whisper-vulkan-sidecar.exe"
    if (-not (Test-Path $BuiltSidecar)) { throw "Whisper Vulkan sidecar not found: $BuiltSidecar" }
    Copy-Item $BuiltSidecar (Join-Path $SidecarOutDir "whisper-vulkan-sidecar-$TargetTriple.exe") -Force
    Copy-Item $BuiltSidecar (Join-Path $SidecarOutDir "whisper-vulkan-sidecar.exe") -Force
}

if (-not $SkipTauriBuild) {
    Write-Step "Building Tauri Store binary"
    $env:VOICETYPR_DISTRIBUTION = "store_msix"
    New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
    try {
        $env:CARGO_ENCODED_RUSTFLAGS = (@($BuildRustFlags) + @("-C", "link-arg=/MAP:$MainMap")) -join [char]0x1f
        pnpm tauri build --no-bundle --ci --config src-tauri/tauri.windows.store.conf.json
        if ($LASTEXITCODE -ne 0) { throw "Tauri Store build failed with exit code $LASTEXITCODE" }
    } finally {
        $env:CARGO_ENCODED_RUSTFLAGS = $OriginalEncodedRustFlags
    }
}

Write-Step "Staging MSIX layout"
$StageDir = Join-Path $RepoRoot "target\store-msix\stage"
$AssetsDir = Join-Path $StageDir "Assets"
Remove-Item $StageDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $AssetsDir | Out-Null

$MainExe = Join-Path $TargetDir "release\voicetypr.exe"
if (-not (Test-Path $MainExe)) { throw "Voicetypr release binary not found: $MainExe" }
Copy-Item $MainExe (Join-Path $StageDir "voicetypr.exe") -Force

# --no-bundle and Store's empty resources list bypass Tauri's bundle hook.
# Stage the same pinned dependency and notices explicitly, then verify them.
node scripts/stage-windows-ort.cjs --msix-layout $StageDir --target-dir $TargetDir --triple $TargetTriple
if ($LASTEXITCODE -ne 0) { throw "ORT MSIX layout validation failed." }
& (Join-Path $RepoRoot "src-tauri/windows/assert-no-onnxruntime-import.ps1") -ExePath (Join-Path $StageDir "voicetypr.exe") -MapPath $MainMap

$Sidecars = @(
    "sidecar\whisper-vulkan\dist\whisper-vulkan-sidecar.exe",
    "sidecar\whisper-vulkan\dist\whisper-vulkan-sidecar-$TargetTriple.exe"
)

foreach ($RelativePath in $Sidecars) {
    $Source = Join-Path $RepoRoot $RelativePath
    if (-not (Test-Path $Source)) { throw "Required Store sidecar missing: $Source" }
    Copy-Item $Source (Join-Path $StageDir (Split-Path -Leaf $Source)) -Force
}

Write-Step "Bundling Visual C++ runtime (app-local)"
# The Store MSIX must be self-contained: it cannot rely on a machine-wide
# Visual C++ Redistributable being present. The main binary is built with the
# static CRT, but whisper-rs enables OpenMP on Windows, which dynamically links
# vcomp140.dll (a Visual C++ Redistributable component with no static MSVC
# variant). We deploy the redistributable DLLs next to voicetypr.exe so they
# resolve from the package
# directory ("local deployment"). This integrates the dependency, keeps the app
# runnable on machines without the redistributable installed, and satisfies
# Microsoft Store policy 10.2.4.1 without a description disclosure.
# Shared with the direct installer build; stages CRT + OpenMP DLLs into a temp
# folder and fails if vcomp140.dll or vcruntime140.dll is missing.
$VcRuntimeDir = Join-Path ([System.IO.Path]::GetTempPath()) "voicetypr-vc-runtime-msix"
& (Join-Path $PSScriptRoot "stage-windows-vc-runtime.ps1") -OutDir $VcRuntimeDir -Prerelease
Get-ChildItem -Path $VcRuntimeDir -Filter "*.dll" | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $StageDir $_.Name) -Force
}

# vcomp140.dll is the load-bearing one (whisper OpenMP). Fail loudly if missing.
if (-not (Test-Path (Join-Path $StageDir "vcomp140.dll"))) {
    throw "vcomp140.dll was not bundled into the MSIX stage; the whisper-rs OpenMP runtime requires it."
}
Write-Success "Bundled Visual C++ runtime DLLs (CRT + OpenMP) into stage"

$IconCopies = @{
    "StoreLogo.png" = "src-tauri\icons\StoreLogo.png"
    "Square44x44Logo.png" = "src-tauri\icons\Square44x44Logo.png"
    "Square71x71Logo.png" = "src-tauri\icons\Square71x71Logo.png"
    "Square150x150Logo.png" = "src-tauri\icons\Square150x150Logo.png"
    "Square310x310Logo.png" = "src-tauri\icons\Square310x310Logo.png"
}

foreach ($Name in $IconCopies.Keys) {
    $Source = Join-Path $RepoRoot $IconCopies[$Name]
    if (-not (Test-Path $Source)) { throw "Required MSIX asset missing: $Source" }
    Copy-Item $Source (Join-Path $AssetsDir $Name) -Force
}

$PackageVersion = node -p "require('./package.json').version + '.0'"
$ManifestSource = Join-Path $RepoRoot "src-tauri\msix\Package.appxmanifest"
$ManifestDest = Join-Path $StageDir "Package.appxmanifest"
$Manifest = Get-Content $ManifestSource -Raw
$Manifest = [regex]::Replace(
    $Manifest,
    '(?m)^(\s*Version=")[^"]+(")',
    ('${1}' + $PackageVersion + '${2}')
)
if ($Manifest -notmatch ('(?m)^\s*Version="' + [regex]::Escape($PackageVersion) + '"')) {
    throw "Failed to set MSIX package identity version to $PackageVersion."
}
if ($Manifest -notmatch 'MinVersion="10\.0\.19041\.0"') {
    throw "MSIX TargetDeviceFamily MinVersion must remain 10.0.19041.0 for Partner Center."
}
$Utf8NoBom = New-Object System.Text.UTF8Encoding -ArgumentList $false
[System.IO.File]::WriteAllText($ManifestDest, $Manifest, $Utf8NoBom)

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $RepoRoot "target\store-msix\Voicetypr_${PackageVersion}_x64.msix"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath = Join-Path $RepoRoot $OutputPath
}

# Recheck after every other staging/copy step, including -NoPack runs.
node scripts/stage-windows-ort.cjs --check-layout $StageDir --triple $TargetTriple
if ($LASTEXITCODE -ne 0) { throw "Final MSIX layout is missing an ORT dependency or notice." }

Write-Success "MSIX stage ready: $StageDir"

if ($NoPack) {
    Write-Info "Skipping winapp pack because -NoPack was specified."
    exit 0
}

Write-Step "Packing MSIX"
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutputPath) | Out-Null

$PackArgs = @(
    "pack",
    $StageDir,
    "--manifest",
    $ManifestDest,
    "--output",
    $OutputPath,
    "--executable",
    "voicetypr.exe"
)

if ([string]::IsNullOrWhiteSpace($CertPath)) {
    Write-Info "Packing with a generated development certificate for CI/local install testing."
    $PackArgs += "--generate-cert"
    $PackArgs += "--install-cert"
} else {
    $ResolvedCertPath = if ([System.IO.Path]::IsPathRooted($CertPath)) {
        $CertPath
    } else {
        Join-Path $RepoRoot $CertPath
    }

    if (-not (Test-Path $ResolvedCertPath)) {
        throw "MSIX certificate not found: $ResolvedCertPath"
    }

    $PackArgs += "--cert"
    $PackArgs += $ResolvedCertPath

    if (-not [string]::IsNullOrWhiteSpace($CertPassword)) {
        $PackArgs += "--cert-password"
        $PackArgs += $CertPassword
    }
}

& winapp @PackArgs
if ($LASTEXITCODE -ne 0) { throw "winapp pack failed with exit code $LASTEXITCODE" }

# Check the actual archive too, not only the pre-pack directory.
$PackageCheckDir = Join-Path ([System.IO.Path]::GetTempPath()) ("voicetypr-msix-check-" + [guid]::NewGuid())
try {
    [System.IO.Compression.ZipFile]::ExtractToDirectory($OutputPath, $PackageCheckDir)
    node scripts/stage-windows-ort.cjs --check-layout $PackageCheckDir --triple $TargetTriple
    if ($LASTEXITCODE -ne 0) { throw "Packed MSIX is missing an ORT dependency or notice." }
} finally {
    Remove-Item $PackageCheckDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Success "MSIX package created: $OutputPath"
