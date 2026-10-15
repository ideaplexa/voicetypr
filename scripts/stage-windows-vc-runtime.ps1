# Stage the Visual C++ runtime DLLs that voicetypr.exe needs, for app-local
# deployment next to the executable (no machine-wide vc_redist install).
#
# The Windows build uses the static CRT (-C target-feature=+crt-static), but
# whisper-rs enables OpenMP on Windows, which dynamically links vcomp140.dll (a
# Visual C++ Redistributable component with no static MSVC variant). Microsoft
# permits "local deployment" of these DLLs from the Visual Studio redist folder.
#
# Used by:
#   - the direct NSIS installer: src-tauri/windows/installer-hooks.nsh copies the
#     bundled DLLs beside voicetypr.exe;
#   - the Store MSIX: scripts/build-msix-store.ps1 (passes -Prerelease).
#
# Usage: pwsh scripts/stage-windows-vc-runtime.ps1 -OutDir src-tauri/windows/resources/vcredist

param(
    [Parameter(Mandatory = $true)]
    [string]$OutDir,

    # Also consider preview/prerelease Visual Studio installs (Store script keeps
    # its historical behaviour). The direct installer build must not.
    [switch]$Prerelease
)

$ErrorActionPreference = 'Stop'

if ($env:OS -ne 'Windows_NT') {
    throw 'Staging the Visual C++ runtime must run on Windows.'
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) {
    throw "vswhere.exe not found: $vswhere (Visual Studio with the C++ build tools is required)."
}

$vswhereArgs = @('-latest')
if ($Prerelease) { $vswhereArgs += '-prerelease' }
$vswhereArgs += @('-property', 'installationPath')
$vsInstallPath = (& $vswhere @vswhereArgs | Select-Object -First 1)
if ([string]::IsNullOrWhiteSpace($vsInstallPath)) {
    throw 'Could not locate a Visual Studio installation via vswhere.'
}

$redistRoot = Join-Path $vsInstallPath 'VC\Redist\MSVC'
if (-not (Test-Path -LiteralPath $redistRoot)) {
    throw "VC redist root not found: $redistRoot (install the 'C++ Redistributable' VS component)."
}

$redistVersionDir = Get-ChildItem -LiteralPath $redistRoot -Directory |
    Where-Object { $_.Name -match '^\d+(\.\d+)+$' } |
    Sort-Object { [version]$_.Name } -Descending |
    Select-Object -First 1
if (-not $redistVersionDir) {
    throw "No versioned VC redist directory under: $redistRoot"
}

$redistX64 = Join-Path $redistVersionDir.FullName 'x64'
$dllDirs = @(
    (Get-ChildItem -LiteralPath $redistX64 -Directory -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue | Select-Object -First 1),
    (Get-ChildItem -LiteralPath $redistX64 -Directory -Filter 'Microsoft.VC*.OpenMP' -ErrorAction SilentlyContinue | Select-Object -First 1)
)

# Start from an empty directory so a stale file from an older redist never ships.
if (Test-Path -LiteralPath $OutDir) {
    Get-ChildItem -LiteralPath $OutDir -File -Filter '*.dll' | Remove-Item -Force
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

foreach ($dllDir in $dllDirs) {
    if (-not $dllDir) {
        throw "Missing VC redist subfolder under $redistX64 (expected Microsoft.VC*.CRT and Microsoft.VC*.OpenMP)."
    }
    Get-ChildItem -LiteralPath $dllDir.FullName -File -Filter '*.dll' | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $OutDir $_.Name) -Force
    }
}

# vcomp140.dll is the load-bearing one (whisper OpenMP); vcruntime140.dll anchors
# the CRT set. Fail loudly if either is missing.
foreach ($required in @('vcomp140.dll', 'vcruntime140.dll')) {
    if (-not (Test-Path -LiteralPath (Join-Path $OutDir $required))) {
        throw "$required was not staged into $OutDir (redist: $($redistVersionDir.FullName))."
    }
}

$staged = @(Get-ChildItem -LiteralPath $OutDir -File -Filter '*.dll' | Sort-Object Name)
Write-Host "Visual C++ redist version: $($redistVersionDir.Name)"
Write-Host "Staged $($staged.Count) runtime DLL(s) into ${OutDir}:"
foreach ($f in $staged) { Write-Host "  $($f.Name)" }
