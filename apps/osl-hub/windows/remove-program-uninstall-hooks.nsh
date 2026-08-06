!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "OSL Privacy: running windows-remove-program-uninstall"
  IfFileExists "$INSTDIR\osl-privacy-hub.exe" 0 osl_windows_remove_program_uninstall_missing
  ExecWait '"$INSTDIR\osl-privacy-hub.exe" --osl-windows-remove-program-uninstall-step' $0
  StrCmp $0 0 osl_windows_remove_program_uninstall_done 0
  Abort "OSL Privacy uninstall cleanup failed."
osl_windows_remove_program_uninstall_missing:
  Abort "OSL Privacy uninstall cleanup is unavailable."
osl_windows_remove_program_uninstall_done:
!macroend
