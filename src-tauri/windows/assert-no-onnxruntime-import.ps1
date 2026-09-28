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
