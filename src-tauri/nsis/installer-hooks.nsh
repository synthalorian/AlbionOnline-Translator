; Post-install hook: ensure the Npcap DRIVER exists.
;
; Why download-at-install instead of bundling: Npcap's installer may not be
; redistributed without a paid OEM license (Wireshark ships it under a special
; agreement). Downloading the pinned, known-good build from the official
; server during setup gives the same one-click result legally.
;
; The bundled wpcap.dll/Packet.dll only let the exe START — real capture needs
; the kernel driver/service, which only the full Npcap install provides.
; Pinned to 1.78 to match the DLLs we bundle (see release.yml).

!include LogicLib.nsh

!define NPCAP_VERSION "1.78"
!define NPCAP_URL "https://npcap.com/dist/npcap-${NPCAP_VERSION}.exe"
!define NPCAP_TMP "$TEMP\npcap-${NPCAP_VERSION}.exe"

!macro NSIS_HOOK_POSTINSTALL
  ; 64-bit registry view — the npcap service key lives in the 64-bit hive.
  SetRegView 64
  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\npcap" "Start"
  ${If} ${Errors}
    DetailPrint "Npcap driver not found — downloading pinned installer (${NPCAP_URL})…"
    nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Invoke-WebRequest -Uri ''${NPCAP_URL}'' -OutFile ''${NPCAP_TMP}''"'
    Pop $1
    ${If} ${FileExists} "${NPCAP_TMP}"
      DetailPrint "Installing Npcap ${NPCAP_VERSION} (silent, default options)…"
      ExecWait '"${NPCAP_TMP}" /S' $2
      ${If} $2 == 0
        DetailPrint "Npcap installed successfully."
      ${Else}
        DetailPrint "Npcap installer exited with code $2."
        MessageBox MB_ICONEXCLAMATION "Npcap setup didn't complete (code $2).$\r$\n$\r$\nPacket capture needs the Npcap driver — install it manually from https://npcap.com (default options), then restart Albion Translator."
      ${EndIf}
      Delete "${NPCAP_TMP}"
    ${Else}
      DetailPrint "Npcap download failed (offline?)."
      MessageBox MB_ICONEXCLAMATION "Couldn't download Npcap automatically.$\r$\n$\r$\nPacket capture needs the Npcap driver — install it from https://npcap.com (default options), then restart Albion Translator."
    ${EndIf}
  ${Else}
    DetailPrint "Npcap driver already present (service registered) — skipping."
  ${EndIf}
!macroend
