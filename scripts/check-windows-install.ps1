# CI check for the Windows NSIS installer: silent install, on-disk dependency
# check, launch check, silent uninstall.
#
# What this proves:
#   - the installer installs silently into a chosen directory (currentUser mode);
#   - the app-local Visual C++ runtime DLLs are copied beside voicetypr.exe;
#   - every DLL imported by voicetypr.exe, the sidecar exe(s) and each DLL in the
#     install root is either a Windows system DLL (allowlist below) or ships in
#     the install directory. This is the real proof that no machine-wide
#     Visual C++ Redistributable is needed: vcruntime*, msvcp*, vcomp*, concrt*
#     and friends are NOT allowlisted, so they must be on disk;
#   - voicetypr.exe starts and exits 0 for --version (the loader resolved all
#     imports, including vcomp140.dll);
#   - the silent uninstaller removes the runtime DLLs it copied.
#
# What it does NOT prove: the GitHub runner has a machine-wide Visual C++
# Redistributable (System32\vcruntime140.dll, vcomp140.dll), so a successful
# launch alone would pass even if the DLLs were not bundled. A fresh Windows VM
# without VC++ is still NEEDS-SMOKE (see plans/082 B.2). Also not covered: the
# Vulkan sidecar actually starting (the runner has no GPU / Vulkan loader), the
# WebView2 window, and upgrade-over-previous-install.
#
# Usage: pwsh scripts/check-windows-install.ps1 -Installer <path-to-setup.exe> [-InstallDir <dir>]

param(
    [Parameter(Mandatory = $true)]
    [string]$Installer,

    [string]$InstallDir = ''
)

$ErrorActionPreference = 'Stop'

if ($env:OS -ne 'Windows_NT') {
    throw 'The installer check must run on Windows.'
}

if (-not (Test-Path -LiteralPath $Installer)) {
    throw "Installer not found: $Installer"
}
$Installer = (Resolve-Path -LiteralPath $Installer).Path

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $base = if (-not [string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) { $env:RUNNER_TEMP } else { $env:TEMP }
    $InstallDir = Join-Path $base 'voicetypr-install-check'
}
$InstallDir = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\')

# Windows system DLLs the build may legitimately import (lowercase, no extension).
# Generous for Windows components, deliberately absent: vcruntime*, msvcp*, vcomp*,
# concrt*, vccorlib*, mfc*, msvcr1xx (Visual C++ runtime; they must ship on disk).
$SystemDlls = @(
    'kernel32', 'kernelbase', 'kernel.appcore', 'ntdll', 'user32', 'gdi32', 'gdi32full',
    'win32u', 'advapi32', 'sechost', 'shell32', 'shcore', 'shlwapi',
    'ole32', 'oleaut32', 'oleacc', 'combase', 'rpcrt4', 'runtimeobject',
    'ws2_32', 'wsock32', 'mswsock', 'dnsapi', 'iphlpapi', 'nsi', 'netapi32', 'wldap32',
    'winhttp', 'wininet', 'urlmon', 'normaliz', 'mpr',
    'bcrypt', 'bcryptprimitives', 'crypt32', 'ncrypt', 'wintrust', 'secur32', 'sspicli', 'credui', 'cryptbase',
    'comctl32', 'comdlg32', 'dwmapi', 'uxtheme', 'msimg32', 'imm32', 'msctf', 'usp10',
    'gdiplus', 'windowscodecs', 'dcomp', 'dwrite', 'd2d1', 'opengl32', 'glu32', 'dxgi',
    'vulkan-1', 'version', 'winmm', 'userenv', 'setupapi', 'cfgmgr32', 'dbghelp', 'dbgcore',
    'powrprof', 'propsys', 'psapi', 'wtsapi32', 'winspool', 'cabinet', 'wevtapi', 'msi',
    'rstrtmgr', 'uiautomationcore', 'hid', 'bthprops', 'xinput1_4', 'xinput9_1_0',
    'avrt', 'ksuser', 'mmdevapi', 'audioses', 'devobj',
    'mf', 'mfplat', 'mfcore', 'mfreadwrite', 'mfsrcsnk', 'mfmediaengine', 'mfsensorgroup',
    'ucrtbase', 'msvcrt', 'msvcp_win', 'wlanapi', 'dhcpcsvc', 'ntmarta', 'winsta', 'wer'
)
$SystemDllPatterns = @(
    '^api-ms-win-.+',
    '^ext-ms-.+',
    '^d3d\d+[a-z0-9_]*$',
    '^d3dcompiler_\d+$'
)

function Test-SystemDll([string]$Name) {
    $n = $Name.ToLowerInvariant() -replace '\.(dll|drv|cpl)$', ''
    if ($SystemDlls -contains $n) { return $true }
    foreach ($pattern in $SystemDllPatterns) {
        if ($n -match $pattern) { return $true }
    }
    return $false
}

function Get-DumpBinPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { return $null }
    $dumpbin = & $vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find '**\Hostx64\x64\dumpbin.exe' 2>$null |
        Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($dumpbin)) { return $null }
    return $dumpbin
}

# Imported DLL names (regular and delay-load) of a PE file, via dumpbin /dependents.
function Get-Dependents([string]$DumpBin, [string]$Path) {
    $output = & $DumpBin /dependents $Path 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin /dependents failed for $Path (exit $LASTEXITCODE): $output"
    }
    $names = New-Object System.Collections.Generic.List[string]
    $inList = $false
    foreach ($line in $output) {
        $text = "$line"
        if ($text -match 'following (delay load )?dependencies') { $inList = $true; continue }
        if ($text -match '^\s*Summary\s*$') { $inList = $false; continue }
        if ($inList -and $text -match '^\s+(\S+)\s*$') { $names.Add($Matches[1]) }
    }
    if ($names.Count -eq 0) {
        throw "Parsed no dependencies from dumpbin output for $Path (the parser is out of date): $output"
    }
    return $names
}

function Invoke-Checked([string]$FilePath, [string]$Arguments, [int]$TimeoutSeconds, [string]$What) {
    $proc = Start-Process -FilePath $FilePath -ArgumentList $Arguments -PassThru
    if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
        try { $proc.Kill() } catch { }
        throw "$What did not finish within $TimeoutSeconds seconds."
    }
    $proc.WaitForExit()
    return $proc.ExitCode
}

function Show-InstallDir {
    if (Test-Path -LiteralPath $InstallDir) {
        Write-Host "Contents of ${InstallDir}:"
        Get-ChildItem -LiteralPath $InstallDir -File | ForEach-Object { Write-Host "  $($_.Name)  ($($_.Length) bytes)" }
    } else {
        Write-Host "Install directory does not exist: $InstallDir"
    }
}

$dumpbin = Get-DumpBinPath
if (-not $dumpbin) {
    throw 'dumpbin.exe not found (install Visual Studio C++ build tools on this runner).'
}

$systemRedist = @('vcruntime140.dll', 'vcomp140.dll') | Where-Object { Test-Path -LiteralPath (Join-Path $env:windir "System32\$_") }
Write-Host "Runner has a machine-wide Visual C++ runtime in System32: $($systemRedist -join ', ')"
Write-Host 'Note: the on-disk dependency check below is the real proof; a successful launch alone would pass on this runner without the bundled DLLs.'

if (Test-Path -LiteralPath $InstallDir) {
    Remove-Item -LiteralPath $InstallDir -Recurse -Force
}

$installed = $false
try {
    # 1. Silent install. /D= must be the last argument and unquoted.
    Write-Host "`n==> Silent install into $InstallDir"
    $code = Invoke-Checked $Installer "/S /D=$InstallDir" 600 'The installer'
    $installed = $true
    if ($code -ne 0) { throw "Installer exited with code $code." }

    $mainExe = Join-Path $InstallDir 'voicetypr.exe'
    if (-not (Test-Path -LiteralPath $mainExe)) { throw "voicetypr.exe was not installed: $mainExe" }

    $sidecars = @(Get-ChildItem -LiteralPath $InstallDir -File -Filter 'whisper-vulkan-sidecar*.exe')
    if ($sidecars.Count -eq 0) { throw 'The Vulkan sidecar exe was not installed.' }

    $bundledDir = Join-Path $InstallDir 'windows\resources\vcredist'
    $staged = @(Get-ChildItem -LiteralPath $bundledDir -File -Filter '*.dll' -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
    if ($staged.Count -eq 0) { throw "No bundled runtime DLLs found in $bundledDir." }
    foreach ($name in $staged) {
        if (-not (Test-Path -LiteralPath (Join-Path $InstallDir $name))) {
            throw "Runtime DLL was not copied beside voicetypr.exe: $name"
        }
    }
    foreach ($required in @('vcomp140.dll', 'vcruntime140.dll')) {
        if ($staged -notcontains $required) { throw "$required is not bundled in the installer." }
    }
    Write-Host "Installed. Runtime DLLs copied beside voicetypr.exe: $($staged -join ', ')"

    # 2. Every import must be a Windows system DLL or ship in the install directory.
    Write-Host "`n==> Dependency check (dumpbin /dependents)"
    $targets = @($mainExe) + @($sidecars | ForEach-Object { $_.FullName }) +
        @(Get-ChildItem -LiteralPath $InstallDir -File -Filter '*.dll' | ForEach-Object { $_.FullName })
    $missing = New-Object System.Collections.Generic.List[string]
    foreach ($target in $targets) {
        $label = Split-Path -Leaf $target
        $fromDisk = @()
        foreach ($dep in (Get-Dependents $dumpbin $target)) {
            if (Test-SystemDll $dep) { continue }
            if (Test-Path -LiteralPath (Join-Path $InstallDir $dep)) { $fromDisk += $dep; continue }
            $missing.Add("$label imports $dep, which is neither a Windows system DLL nor in the install directory")
        }
        $note = if ($fromDisk.Count -gt 0) { " (resolved from install dir: $(($fromDisk | Sort-Object -Unique) -join ', '))" } else { '' }
        Write-Host "  $label$note"
    }
    if ($missing.Count -gt 0) {
        $missing | ForEach-Object { Write-Host "  MISSING: $_" -ForegroundColor Red }
        throw "$($missing.Count) unresolved dependency(ies); the installer would need a machine-wide runtime."
    }

    # 3. Load check. voicetypr.exe is GUI-subsystem, so Start-Process (not &) is
    # needed to wait for it and read its exit code; --version exits before the webview.
    Write-Host "`n==> Launch check: voicetypr.exe --version"
    $code = Invoke-Checked $mainExe '--version' 120 'voicetypr.exe --version'
    if ($code -ne 0) {
        $hint = if ($code -eq -1073741515) { ' (STATUS_DLL_NOT_FOUND: an imported DLL could not be loaded)' } else { '' }
        throw "voicetypr.exe --version exited with code $code$hint."
    }
    Write-Host 'voicetypr.exe --version exited 0.'

    # 4. Silent uninstall. _?= keeps the NSIS uninstaller in-process so Start-Process
    # waits for it (otherwise it relaunches from %TEMP% and returns immediately).
    Write-Host "`n==> Silent uninstall"
    $uninstaller = Join-Path $InstallDir 'uninstall.exe'
    if (-not (Test-Path -LiteralPath $uninstaller)) { throw "uninstall.exe not found: $uninstaller" }
    $code = Invoke-Checked $uninstaller "/S _?=$InstallDir" 300 'The uninstaller'
    $installed = $false
    if ($code -ne 0) { throw "Uninstaller exited with code $code." }

    $leftovers = @()
    foreach ($name in $staged) {
        if (Test-Path -LiteralPath (Join-Path $InstallDir $name)) { $leftovers += $name }
    }
    foreach ($name in @('voicetypr.exe', 'voicetypr.pdb')) {
        if (Test-Path -LiteralPath (Join-Path $InstallDir $name)) { $leftovers += $name }
    }
    if ($leftovers.Count -gt 0) {
        throw "Uninstall left files behind: $($leftovers -join ', ')"
    }
    Write-Host 'Uninstall removed the app and every runtime DLL it copied.'

    Write-Host "`nOK: silent install, on-disk dependency check, launch and uninstall passed."
}
catch {
    Write-Host "`nInstaller check FAILED: $($_.Exception.Message)" -ForegroundColor Red
    Show-InstallDir
    throw
}
finally {
    if ($installed) {
        # Best-effort cleanup after a failure; never mask the original error.
        $uninstaller = Join-Path $InstallDir 'uninstall.exe'
        if (Test-Path -LiteralPath $uninstaller) {
            try { Invoke-Checked $uninstaller "/S _?=$InstallDir" 300 'The uninstaller (cleanup)' | Out-Null } catch { }
        }
    }
    if (Test-Path -LiteralPath $InstallDir) {
        Remove-Item -LiteralPath $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
