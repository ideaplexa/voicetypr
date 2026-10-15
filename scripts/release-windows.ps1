# Windows x86_64 release script - one CPU-safe installer with optional Vulkan GPU sidecar.
# Uses src-tauri/tauri.windows.conf.json, which bundles the x64 Vulkan sidecar only.
# Windows ARM64 builds stay CPU-only and must not use this config or sidecar path.

param(
    [string]$Version,
    [switch]$Help,
    [switch]$SkipBuild,
    [switch]$SkipPublish
)

$GitHubRepository = "ideaplexa/voicetypr"

function Write-Success($Message) { Write-Host "[OK] $Message" -ForegroundColor Green }
function Write-Error($Message) { Write-Host "[ERROR] $Message" -ForegroundColor Red }
function Write-Info($Message) { Write-Host "[INFO] $Message" -ForegroundColor Cyan }
function Write-Step($Message) { Write-Host "`n==> $Message" -ForegroundColor Magenta }

function Require-Command($Command) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        Write-Error "$Command not found in PATH"
        exit 1
    }
}

function Require-File($Path) {
    if (-not (Test-Path $Path)) {
        Write-Error "Required file not found: $Path"
        exit 1
    }
}

if ($Help) {
    Write-Host @"
Windows x86_64 Release Script

Builds one Windows x64 NSIS installer:
- voicetypr.exe is CPU-safe and must not import vulkan-1.dll
- optional Vulkan acceleration ships as an x86_64 sidecar process
- the Visual C++ runtime DLLs (CRT + OpenMP) are bundled and installed app-local; the Vulkan loader comes from the GPU driver
- updater/latest.json points to this single installer

This script and src-tauri/tauri.windows.conf.json are for x86_64 Windows only.
Windows ARM64 builds are CPU-only and must not require the x64 Vulkan sidecar.

Usage:
  .\scripts\release-windows.ps1                    # Build and upload installer
  .\scripts\release-windows.ps1 -SkipBuild         # Upload existing build
  .\scripts\release-windows.ps1 -SkipPublish       # Build only, don't upload
  .\scripts\release-windows.ps1 -Help              # Show this help

Requirements for building:
  - Vulkan SDK in VULKAN_SDK, used only to build/package the x64 GPU sidecar
  - Optional CARGO_TARGET_DIR for short target-specific build paths (honored for sidecar and main app)
"@
    exit 0
}

if (-not $Version) {
    $packageJson = Get-Content "package.json" | ConvertFrom-Json
    $Version = $packageJson.version
}

Write-Step "Voicetypr Windows Release v$Version"

$ReleaseTag = "v$Version"
$OutputDir = "release-windows-$Version"
$InstallerName = "Voicetypr_${Version}_x64-setup.exe"

$WindowsTarget = "x86_64-pc-windows-msvc"
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

Require-Command cargo
Require-Command pnpm
Require-File "package.json"

if (-not $SkipPublish) {
    Require-Command gh
}

if (-not $SkipBuild) {
    if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne [System.Runtime.InteropServices.Architecture]::X64) {
        Write-Error "This release script is only for x86_64 Windows. Windows ARM64 builds must use the CPU-only release path."
        exit 1
    }
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_BUILD_TARGET) -and $env:CARGO_BUILD_TARGET -ne $WindowsTarget) {
        Write-Error "CARGO_BUILD_TARGET must be $WindowsTarget for this release path, got: $env:CARGO_BUILD_TARGET"
        exit 1
    }

    Write-Step "Building x86_64 CPU-safe app with bundled Vulkan sidecar"

    $AppTargetDir = $env:CARGO_TARGET_DIR
    if ([string]::IsNullOrWhiteSpace($AppTargetDir)) {
        $AppTargetDir = "src-tauri\target"
    }

    if ([string]::IsNullOrEmpty($env:VULKAN_SDK) -or -not (Test-Path $env:VULKAN_SDK)) {
        Write-Error "VULKAN_SDK not set. Install Vulkan SDK to build the optional GPU sidecar."
        Write-Info "Download from: https://vulkan.lunarg.com/sdk/home"
        exit 1
    }

    cargo clean --manifest-path src-tauri\Cargo.toml
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    # App-local Visual C++ runtime: CRT + OpenMP DLLs from the Visual Studio redist
    # folder, bundled into the installer and copied beside voicetypr.exe by the
    # NSIS hook. No vc_redist / Vulkan runtime installers are downloaded or run.
    $runtimeDir = "src-tauri\windows\resources\vcredist"
    Write-Info "Staging Visual C++ runtime DLLs..."
    try {
        & (Join-Path $PSScriptRoot "stage-windows-vc-runtime.ps1") -OutDir $runtimeDir
    } catch {
        Write-Error "Staging the Visual C++ runtime failed: $($_.Exception.Message)"
        exit 1
    }
    Write-Info "Building Whisper Vulkan sidecar (x86_64 only)..."
    $SidecarTargetDir = $env:CARGO_TARGET_DIR
    if ([string]::IsNullOrWhiteSpace($SidecarTargetDir)) {
        $SidecarTargetDir = "sidecar\whisper-vulkan\target"
    }

    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    cargo build --manifest-path sidecar\whisper-vulkan\Cargo.toml --release --target $WindowsTarget
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Vulkan sidecar build failed"
        exit $LASTEXITCODE
    }

    $SidecarExe = Join-Path $SidecarTargetDir "$WindowsTarget\release\whisper-vulkan-sidecar.exe"
    if (-not (Test-Path $SidecarExe)) {
        Write-Error "Whisper Vulkan sidecar binary not found after build: $SidecarExe"
        exit 1
    }

    $SidecarDist = "sidecar\whisper-vulkan\dist\whisper-vulkan-sidecar-x86_64-pc-windows-msvc.exe"
    New-Item -ItemType Directory -Path "sidecar\whisper-vulkan\dist" -Force | Out-Null
    Copy-Item $SidecarExe $SidecarDist -Force

    Write-Info "Verifying bundled sidecar and Visual C++ runtime before packaging..."
    Require-File "$runtimeDir\vcomp140.dll"
    Require-File "$runtimeDir\vcruntime140.dll"
    Require-File $SidecarDist
    Write-Success "Bundled Vulkan sidecar + app-local Visual C++ runtime present"

    $env:VOICETYPR_REQUIRE_VULKAN_SIDECAR = "1"

    Write-Info "Building Tauri x86_64 installer..."
    pnpm tauri build --target $WindowsTarget --ci --config src-tauri/tauri.windows.conf.json
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build failed"
        exit $LASTEXITCODE
    }

    $MainExe = Join-Path $AppTargetDir "$WindowsTarget\release\voicetypr.exe"
    powershell -ExecutionPolicy Bypass -File .\src-tauri\windows\assert-no-vulkan-import.ps1 -ExePath $MainExe
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $InstallerDir = Join-Path $AppTargetDir "$WindowsTarget\release\bundle\nsis"
    $installer = Get-ChildItem "$InstallerDir\*.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        Write-Error "No installer found in $InstallerDir\"
        exit 1
    }

    $installerPath = "$OutputDir\$InstallerName"
    Copy-Item $installer.FullName $installerPath -Force
    Write-Success "Installer built: $installerPath"

    $tempSigningKey = $null
    if (-not [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PATH)) {
        $keyPath = $env:TAURI_SIGNING_PRIVATE_KEY_PATH
        if (-not (Test-Path $keyPath)) {
            Write-Error "TAURI_SIGNING_PRIVATE_KEY_PATH does not exist: $keyPath"
            exit 1
        }
    } elseif (-not [string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY)) {
        $tempSigningKey = Join-Path ([System.IO.Path]::GetTempPath()) "tauri-signing-$([System.Guid]::NewGuid()).key"
        [System.IO.File]::WriteAllText($tempSigningKey, $env:TAURI_SIGNING_PRIVATE_KEY)
        $keyPath = $tempSigningKey
    } else {
        $keyPath = "$env:USERPROFILE\.tauri\voicetypr.key"
    }

    $signature = ""
    try {
        if (Test-Path $keyPath) {
            Write-Info "Signing installer for updates..."
            if (-not [string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
                & pnpm tauri signer sign -f $keyPath -p $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD $installerPath
            } else {
                & pnpm tauri signer sign -f $keyPath --password= $installerPath
            }
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

            if (Test-Path "$installerPath.sig") {
                $signature = (Get-Content "$installerPath.sig" -Raw).Trim() -replace "`r`n", "" -replace "`n", ""
                Write-Success "Installer signed"
            } else {
                Write-Error "Failed to sign installer (missing .sig file)"
                exit 1
            }
        } else {
            Write-Error "No signing key found. Set TAURI_SIGNING_PRIVATE_KEY_PATH, TAURI_SIGNING_PRIVATE_KEY, or place the key at $keyPath."
            exit 1
        }
    } finally {
        if ($tempSigningKey) {
            Remove-Item $tempSigningKey -ErrorAction SilentlyContinue
        }
    }

    Write-Info "Updating latest.json with Windows platform..."
    $latestJsonPath = "$OutputDir\latest.json"
    try {
        gh release download $ReleaseTag --repo $GitHubRepository -p "latest.json" -D $OutputDir --clobber 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path $latestJsonPath)) { Write-Success "Downloaded existing latest.json" }
    } catch {
        Write-Info "No existing latest.json downloaded: $_"
    }

    if (Test-Path $latestJsonPath) {
        $latestJson = Get-Content $latestJsonPath -Raw | ConvertFrom-Json
        if (-not $latestJson.platforms) {
            $latestJson | Add-Member -NotePropertyName "platforms" -NotePropertyValue @{} -Force
        }
        foreach ($platform in $latestJson.platforms.PSObject.Properties) {
            if ($platform.Value.url -match '^https://github\.com/[^/]+/voicetypr/releases/download/') {
                $platform.Value.url = $platform.Value.url -replace '^https://github\.com/[^/]+/voicetypr/', "https://github.com/$GitHubRepository/"
            }
        }
        $windowsPlatform = @{
            signature = $signature
            url = "https://github.com/$GitHubRepository/releases/download/$ReleaseTag/$InstallerName"
        }
        $latestJson.platforms | Add-Member -NotePropertyName "windows-x86_64" -NotePropertyValue $windowsPlatform -Force
        $latestJson | ConvertTo-Json -Depth 10 | Set-Content $latestJsonPath
    } else {
        $latestJson = @{
            version = "v$Version"
            notes = "See the release notes for v$Version"
            pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")
            platforms = @{
                "windows-x86_64" = @{
                    signature = $signature
                    url = "https://github.com/$GitHubRepository/releases/download/$ReleaseTag/$InstallerName"
                }
            }
        }
        $latestJson | ConvertTo-Json -Depth 10 | Set-Content $latestJsonPath
    }
}

if ($SkipBuild) {
    if (-not (Test-Path $OutputDir)) {
        Write-Error "Output directory not found: $OutputDir"
        exit 1
    }
    if (-not (Test-Path "$OutputDir\$InstallerName.sig")) {
        Write-Error "No signature file found at $OutputDir\$InstallerName.sig"
        exit 1
    }
}

if (-not $SkipPublish) {
    Write-Step "Uploading to GitHub"
    gh release view $ReleaseTag --repo $GitHubRepository 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Release $ReleaseTag not found. Run macOS release first to create the draft."
        exit 1
    }

    gh release upload $ReleaseTag "$OutputDir\$InstallerName" --repo $GitHubRepository --clobber
    gh release upload $ReleaseTag "$OutputDir\$InstallerName.sig" --repo $GitHubRepository --clobber
    if (Test-Path "$OutputDir\latest.json") {
        gh release upload $ReleaseTag "$OutputDir\latest.json" --repo $GitHubRepository --clobber
    }
    Write-Success "Installer uploaded successfully"
}

Write-Step "Done"
Write-Info "Installer: $InstallerName"
Write-Info "Main app is CPU-safe; optional x86_64 GPU acceleration is isolated in the bundled sidecar."
