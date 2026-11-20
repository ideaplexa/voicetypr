; Windows installer hook: app-local VC runtime + CPU ONNX Runtime + symbols.
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
    ${OrIfNot} ${FileExists} "$INSTDIR\onnxruntime.dll"
        DetailPrint "Visual C++ runtime files are missing"
        ${If} ${Silent}
            SetErrorLevel 2
        ${Else}
            MessageBox MB_ICONSTOP|MB_OK "Voicetypr could not install its runtime files. Please reinstall." /SD IDOK
        ${EndIf}
        Abort
    ${EndIf}
    ; Never leave the obsolete DirectML dependency behind on an upgrade.
    Delete "$INSTDIR\DirectML.dll"
    Delete "$INSTDIR\windows\resources\vcredist\DirectML.dll"
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

!macro RemoveLegacyFfmpeg
    ; Installers before 2.1 shipped ffmpeg sidecars that an older bundler named
    ; ffmpeg.exe.exe / ffprobe.exe.exe (about 100 MB each). Nothing uses them now.
    Delete "$INSTDIR\ffmpeg.exe.exe"
    Delete "$INSTDIR\ffprobe.exe.exe"
!macroend

!define CLI_PATH_MARKER_KEY "Software\Voicetypr\CliPath"

!macro NSIS_HOOK_POSTINSTALL
    !insertmacro InstallVcRuntime
    !insertmacro InstallPdbSymbols
    !insertmacro RemoveLegacyFfmpeg

    ; A replacing install runs the old uninstaller first, which took the CLI's
    ; PATH entry with it and left this marker: put the entry back.
    Push $0
    ClearErrors
    ReadRegDWORD $0 HKCU "${CLI_PATH_MARKER_KEY}" "RestoreOnInstall"
    ${If} $0 = 1
        ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" restore-cli-path'
        DeleteRegKey HKCU "${CLI_PATH_MARKER_KEY}"
    ${EndIf}
    Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
    ; This hook runs before Tauri's own running-app check; stop the app before deleting loaded DLLs.
    !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

    ; Uninstall takes the CLI's user PATH entry with it; an in-app update (/UPDATE)
    ; keeps it. The app edits HKCU\Environment itself, which avoids NSIS PATH
    ; truncation. Exit 0 means an entry was removed: remember it, so a manual
    ; upgrade (which uninstalls first) restores it in POSTINSTALL.
    ${If} $UpdateMode <> 1
        Push $0
        ClearErrors
        ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" remove-cli-path' $0
        ${IfNot} ${Errors}
        ${AndIf} $0 = 0
            WriteRegDWORD HKCU "${CLI_PATH_MARKER_KEY}" "RestoreOnInstall" 1
        ${EndIf}
        Pop $0
    ${EndIf}

    Delete "$INSTDIR\voicetypr.pdb"
    !insertmacro RemoveLegacyFfmpeg

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
