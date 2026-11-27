; KEL-270 Windows v2 PerUserDirect NSIS packaging input.
; The builder defines every KELD_* source from independently pinned release inputs.
; Only the original signed Host may interpret the commissioned catalogue, choose
; or provision an installation root, and admit the first protected effect.
; This NSIS layer writes only an ephemeral extraction directory, never an
; installation root, registry owner, service, ACL, updater record or shortcut.
;
; Source compilation is blocked until the native T81/T20/D90/D46 owner issues
; an authenticated build admission. An NSIS binary, if later produced, still
; needs independent production signing and native installed acceptance.

Unicode true
Name "Keld Windows v2 PerUserDirect"
OutFile "${KELD_UNSIGNED_OUTPUT}"
RequestExecutionLevel user
SilentInstall silent
AutoCloseWindow true
CRCCheck force
SetCompressor /SOLID lzma

; Never write to $INSTDIR. In particular /D= does not select an installed root.
InstallDir ""

Section "Keld PerUserDirect initial installation"
  SetShellVarContext current
  InitPluginsDir
  StrCpy $1 "$PLUGINSDIR\keld-v2"

  SetOutPath "$1\bin"
  File "/oname=keld-host.exe" "${KELD_APP_HOST}"
  File "/oname=keld.exe" "${KELD_CLI}"
  File "/oname=keld-role-launcher.exe" "${KELD_ROLE_LAUNCHER}"
  File "/oname=keld-updater-helper.exe" "${KELD_UPDATER_HELPER}"

  SetOutPath "$1\recovery"
  File "/oname=keld-host.exe" "${KELD_RECOVERY_HOST}"

  ; Keep the existing W38 provider recipe available to the authenticated Host.
  ; NSIS does not register a provider or select a permanent CLI path itself.
  SetOutPath "$1\bindings"
  File "/oname=keld-mcp-stdio.json" "${KELD_PROVIDER_RECIPE}"

  SetOutPath "$1\catalogue"
  File "/oname=commissioned-catalogue.bin" "${KELD_CATALOGUE}"

  ; A request locator is never trust authority. The original signed Host must
  ; authenticate the commissioned catalogue and bind T81/T20/D90/D46 itself.
  ; The currently published Host deliberately refuses this private role.
  ClearErrors
  ExecWait '"$1\bin\keld-host.exe" --keld-install-initial-v3 "$1\catalogue\commissioned-catalogue.bin"' $0
  IfErrors refused
  IntCmp $0 0 returned_zero refused refused

returned_zero:
  ; A process exit code is not native installed acceptance or release evidence.
  DetailPrint "Original Host initial-install returned zero. Native installed acceptance remains independent."
  Goto complete

refused:
  SetErrors
  Abort "KEL270-NSIS: original signed Host refused the initial install or could not be launched."

complete:
SectionEnd
