; Post-install hook: ensure the Npcap DRIVER exists.
;
; Why download-at-install instead of bundling: Npcap's installer may not be
; redistributed without a paid OEM license (Wireshark ships it under a special
; agreement). Downloading the pinned, known-good build from the official
; server during setup gives the same one-click result legally.
;
; The bundled wpcap.dll/Packet.dll only let the exe START - real capture needs
; the kernel driver/service, which only the full Npcap install provides.
; Pinned to 1.78 to match the DLLs we bundle (see release.yml).

!include LogicLib.nsh

!define NPCAP_VERSION "1.78"
!define NPCAP_URL "https://npcap.com/dist/npcap-${NPCAP_VERSION}.exe"
!define NPCAP_TMP "$TEMP\npcap-${NPCAP_VERSION}.exe"

!macro NSIS_HOOK_POSTINSTALL
  ; 64-bit registry view - the npcap service key lives in the 64-bit hive.
  ; ClearErrors before ReadRegStr so an earlier non-fatal error flag doesn't
  ; falsely trigger Npcap installation when the driver is already registered.
  SetRegView 64
  ClearErrors
  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\npcap" "Start"
  SetRegView lastused
  ${If} ${Errors}
  ${OrIf} $0 == ""
    ; Free Npcap installers reject /S ("Silent installation is only supported
    ; in Npcap OEM", exit code 2), so installation must show Npcap's GUI.
    ; Skip interactive prompts/dialogs during passive or silent updater runs.
    ${If} $PassiveMode = 1
    ${OrIf} ${Silent}
      DetailPrint "Npcap driver not found; skipping interactive Npcap setup in silent/passive mode."
    ${Else}
      DetailPrint "Npcap driver not found - downloading pinned installer (${NPCAP_URL})..."
      ; Pass URL and destination via environment variables so spaces or
      ; apostrophes in $TEMP (e.g. C:\Users\O'Brien\...) never break quoting,
      ; and use backticks in NSIS so PowerShell's quotes stay intact.
      System::Call 'Kernel32::SetEnvironmentVariable(t "ALBION_NPCAP_URL", t "${NPCAP_URL}")'
      System::Call 'Kernel32::SetEnvironmentVariable(t "ALBION_NPCAP_TMP", t "${NPCAP_TMP}")'
      nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$ProgressPreference = 'SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing -Uri $$env:ALBION_NPCAP_URL -OutFile $$env:ALBION_NPCAP_TMP"`
      Pop $1
      ${If} $1 == "0"
      ${AndIf} ${FileExists} "${NPCAP_TMP}"
        DetailPrint "Launching Npcap ${NPCAP_VERSION} installer (accept UAC and follow prompts)..."
        ; ExecShellWait triggers UAC elevation (needed when Tauri's NSIS runs in
        ; currentUser mode) without passing /S (which free Npcap rejects).
        ClearErrors
        ExecShellWait "" "${NPCAP_TMP}"
        Delete "${NPCAP_TMP}"
        SetRegView 64
        ClearErrors
        ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\npcap" "Start"
        SetRegView lastused
        ${If} ${Errors}
        ${OrIf} $0 == ""
          DetailPrint "Npcap service not registered after installer exited."
          MessageBox MB_ICONEXCLAMATION "Npcap setup didn't complete.$\r$\n$\r$\nPacket capture needs the Npcap driver - install it manually from https://npcap.com (default options), then restart Albion Translator."
        ${Else}
          DetailPrint "Npcap installed successfully."
        ${EndIf}
      ${Else}
        Delete "${NPCAP_TMP}"
        DetailPrint "Npcap download failed (exit code $1)."
        MessageBox MB_ICONEXCLAMATION "Couldn't download Npcap automatically.$\r$\n$\r$\nPacket capture needs the Npcap driver - install it from https://npcap.com (default options), then restart Albion Translator."
      ${EndIf}
    ${EndIf}
  ${Else}
    DetailPrint "Npcap driver already present (service registered) - skipping."
  ${EndIf}
!macroend
