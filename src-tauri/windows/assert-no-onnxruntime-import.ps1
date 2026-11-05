# Fail if the Windows app dynamically imports ONNX Runtime instead of linking it statically.
param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $ExePath)) {
    throw "Executable not found: $ExePath"
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) {
    throw "vswhere.exe not found: $vswhere"
}

$dumpbin = & $vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find '**\Hostx64\x64\dumpbin.exe' 2>$null |
    Select-Object -First 1
if ([string]::IsNullOrWhiteSpace($dumpbin)) {
    throw 'dumpbin.exe not found (install Visual Studio C++ build tools on this runner)'
}

$imports = & $dumpbin /imports $ExePath 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "dumpbin failed for $ExePath (exit $LASTEXITCODE): $imports"
}

if (@($imports | Where-Object { $_ -match '(?i)^\s*onnxruntime\.dll\s*$' }).Count -gt 0) {
    throw "App must not import onnxruntime.dll: $ExePath"
}

Write-Host "OK: $ExePath does not import onnxruntime.dll"

# CPU-only sessions still use the pinned DirectML-enabled static prebuilt.
# This check covers both users: Parakeet TDT and Silero VAD.
if (@($imports | Where-Object { $_ -match '(?i)^\s*DirectML\.dll\s*$' }).Count -eq 0) {
    throw 'Pinned ORT unexpectedly has no DirectML import. Review and remove staging if a CPU-only build replaced it.'
}
$dll = Join-Path (Split-Path -Parent $ExePath) 'DirectML.dll'
$manifest = Get-Content (Join-Path $PSScriptRoot '../models/onnxruntime/DirectML-provenance.json') -Raw | ConvertFrom-Json
if (-not (Test-Path -LiteralPath $dll)) { throw 'App-local DirectML.dll is missing.' }
if ((Get-FileHash $dll -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.dll_sha256.'x86_64-pc-windows-msvc') {
    throw 'App-local DirectML.dll does not match Microsoft.AI.DirectML 1.15.4.'
}
Write-Host 'OK: static ORT imports the pinned app-local Microsoft DirectML redistributable.'
