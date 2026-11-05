# ORT must be loaded dynamically after the CPU guard, never imported or static.
param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath,
    [Parameter(Mandatory = $true)]
    [string]$MapPath
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    throw "Executable not found: $ExePath"
}
if (-not (Test-Path -LiteralPath $MapPath -PathType Leaf)) {
    throw "Main executable linker MAP not found: $MapPath"
}

# Do not accept a build-script/sidecar MAP, or a stale MAP from another link.
# /MAP's first nonempty line is the image name (normally without extension).
$map = Get-Content -LiteralPath $MapPath -Raw
$image = ($map -split '\r?\n' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -First 1).Trim()
# Cargo links release/deps/voicetypr-<metadata hash>.exe, then copies it to
# release/voicetypr.exe. MSVC's MAP retains that original hashed image name.
$mainName = [System.IO.Path]::GetFileNameWithoutExtension($ExePath)
$imagePattern = '(?i)^' + [regex]::Escape($mainName) + '(?:-[0-9a-f]{16})?$'
if ([System.IO.Path]::GetFileNameWithoutExtension($image) -notmatch $imagePattern) {
    throw 'Linker MAP does not describe the main executable.'
}
$pe = [System.IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $ExePath).Path)
if ($pe.Length -lt 64 -or [System.Text.Encoding]::ASCII.GetString($pe, 0, 2) -ne 'MZ') {
    throw 'Main executable has no valid PE header.'
}
$header = [System.BitConverter]::ToUInt32($pe, 0x3c)
if ($header -lt 64 -or $header + 64 -gt $pe.Length -or
    [System.BitConverter]::ToUInt32($pe, $header) -ne 0x4550 -or
    [System.BitConverter]::ToUInt16($pe, $header + 4) -ne 0x8664 -or
    [System.BitConverter]::ToUInt16($pe, $header + 24) -ne 0x20b) {
    throw 'Main executable must be a Windows x64 PE32+ image.'
}
$timestamp = [regex]::Match($map, '(?im)^\s*Timestamp is ([0-9a-f]{1,8})(?:\s|$)')
$base = [regex]::Match($map, '(?im)^\s*Preferred load address is ([0-9a-f]{8,16})\s*$')
if (-not $timestamp.Success -or -not $base.Success -or
    [Convert]::ToUInt32($timestamp.Groups[1].Value, 16) -ne [System.BitConverter]::ToUInt32($pe, $header + 8) -or
    [Convert]::ToUInt64($base.Groups[1].Value, 16) -ne [System.BitConverter]::ToUInt64($pe, $header + 48)) {
    throw 'Linker MAP timestamp/load address does not match the main executable.'
}
$publics = [regex]::Match($map, '(?is)\bAddress\s+Publics by Value\s+Rva\+Base\s+Lib:Object\s*\r?\n(?<symbols>.*?)(?:\bentry point at\b)')
if (-not $publics.Success -or
    $publics.Groups['symbols'].Value -notmatch '(?im)^\s*[0-9a-f]{4}:[0-9a-f]{8,16}\s+\S+\s+[0-9a-f]{8,16}(?:\s|$)') {
    throw 'Main executable linker MAP has no complete public-symbol table.'
}
# A dynamic loader legitimately contains the string "OrtGetApiBase". Only a
# native, unmangled public symbol definition proves static ORT was linked.
if ($publics.Groups['symbols'].Value -match '(?im)^\s*[0-9a-f]{4}:[0-9a-f]{8,16}\s+OrtGetApiBase\s+[0-9a-f]{8,16}(?:\s|$)') {
    throw 'Main executable statically defines OrtGetApiBase; static ONNX Runtime is forbidden.'
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
if (@($imports | Where-Object { $_ -match '(?i)^\s*(?:onnxruntime[^\s]*|DirectML)\.dll\s*$' }).Count -gt 0) {
    throw 'Main executable must not natively import ONNX Runtime or DirectML (including delay imports).'
}
Write-Host 'OK: main-image MAP and imports exclude static ORT and native ORT/DirectML imports.'
