!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy AllSigned -File "$INSTDIR\native-host\install-native-host-release.ps1" -HostPath "$INSTDIR\verisilo-native-host.exe" -ReleaseConfigPath "$INSTDIR\native-host\native-host-release-config.json"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$1"
    MessageBox MB_ICONSTOP|MB_OK "VeriSilo Native Messaging Host registration failed. The installer will stop without reporting a successful installation."
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy AllSigned -File "$INSTDIR\native-host\uninstall-native-host.ps1"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$1"
    MessageBox MB_ICONSTOP|MB_OK "VeriSilo could not safely remove its current-user Native Messaging registration. Uninstall has been stopped; Vault and Silo Profile data were not touched."
    Abort
  ${EndIf}
!macroend
