; Windows installer hook: app-local Visual C++ runtime DLLs + debug symbols.
; Voicetypr's main executable is CPU-safe; Vulkan is isolated in a sidecar.

!include "LogicLib.nsh"

!macro InstallVcRuntime
    ; Visual C++ runtime, deployed app-local. voicetypr.exe uses the static CRT,
    ; but whisper-rs OpenMP imports vcomp140.dll (no static variant). The build
    ; bundles the redist DLLs (scripts/stage-windows-vc-runtime.ps1); copy them
    ; beside voicetypr.exe, which precedes System32 in the DLL search order. No
    ; machine-wide vc_redist install, no network, no admin rights.
    ; The Vulkan loader (vulkan-1.dll) is deliberately NOT installed: GPU drivers
    ; ship it, and the app falls back to CPU with a clear message without it.
    DetailPrint "Installing Visual C++ runtime files..."
    Push $R0
    Push $R1
    FindFirst $R0 $R1 "$INSTDIR\windows\resources\vcredist\*.dll"
    vcrt_copy_loop:
        StrCmp $R1 "" vcrt_copy_done
        CopyFiles /SILENT "$INSTDIR\windows\resources\vcredist\$R1" "$INSTDIR\$R1"
        FindNext $R0 $R1
        Goto vcrt_copy_loop
    vcrt_copy_done:
    FindClose $R0
    Pop $R1
    Pop $R0

    ${IfNot} ${FileExists} "$INSTDIR\vcomp140.dll"
    ${OrIfNot} ${FileExists} "$INSTDIR\vcruntime140.dll"
        DetailPrint "Visual C++ runtime files are missing"
        ${If} ${Silent}
            SetErrorLevel 2
        ${Else}
            MessageBox MB_ICONSTOP|MB_OK "Voicetypr could not install its runtime files. Please reinstall." /SD IDOK
        ${EndIf}
        Abort
    ${EndIf}
!macroend

!macro InstallPdbSymbols
    ; Place debug symbols (voicetypr.pdb) beside voicetypr.exe so the PostHog
    ; backtrace integration (dbghelp) resolves function names + line numbers at
    ; crash time. The bundler installs it under windows/resources/; copy it next to the
    ; exe where dbghelp's default per-module search looks. Best-effort: symbols
    ; are a diagnostics aid and never required for the app to run.
    ${If} ${FileExists} "$INSTDIR\windows\resources\voicetypr.pdb"
        DetailPrint "Installing debug symbols (voicetypr.pdb)..."
        CopyFiles /SILENT "$INSTDIR\windows\resources\voicetypr.pdb" "$INSTDIR\voicetypr.pdb"
    ${Else}
        DetailPrint "voicetypr.pdb not bundled, skipping debug symbol install"
    ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
    !insertmacro InstallVcRuntime
    !insertmacro InstallPdbSymbols
!macroend

!macro NSIS_HOOK_PREUNINSTALL
    Delete "$INSTDIR\voicetypr.pdb"

    ; Remove the app-local runtime DLLs copied at install time. Iterate the
    ; bundled folder (still present at this point) so the list never drifts.
    Push $R0
    Push $R1
    FindFirst $R0 $R1 "$INSTDIR\windows\resources\vcredist\*.dll"
    vcrt_remove_loop:
        StrCmp $R1 "" vcrt_remove_done
        Delete "$INSTDIR\$R1"
        FindNext $R0 $R1
        Goto vcrt_remove_loop
    vcrt_remove_done:
    FindClose $R0
    Pop $R1
    Pop $R0
!macroend
