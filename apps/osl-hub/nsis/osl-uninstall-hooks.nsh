!macro OSL_WRITE_IDENTITY_BACKUP IDENTITY_SOURCE
  IfFileExists "${IDENTITY_SOURCE}" 0 +3
    CreateDirectory "$DOCUMENTS"
    CopyFiles /SILENT "${IDENTITY_SOURCE}" "$DOCUMENTS\OSL identity backup.json"
!macroend

!macro OSL_WRITE_ACTIVE_SLOT_BACKUP
  ClearErrors
  FileOpen $0 "$APPDATA\org.oslprivacy.hub\osl-core\hub-active-identity" r
  IfErrors done
  FileRead $0 $1
  FileClose $0
  !insertmacro OSL_WRITE_IDENTITY_BACKUP "$APPDATA\org.oslprivacy.hub\osl-core\hub-identities\$1\identity.json"
done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  IfSilent remove_backup 0
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Write one OSL identity backup to your Documents folder before uninstall removes local OSL data?" \
    IDYES keep_backup IDNO remove_backup

keep_backup:
  IfFileExists "$APPDATA\org.oslprivacy.hub\osl-core\identity.json" 0 +2
    !insertmacro OSL_WRITE_IDENTITY_BACKUP "$APPDATA\org.oslprivacy.hub\osl-core\identity.json"
  IfFileExists "$DOCUMENTS\OSL identity backup.json" cleanup
  !insertmacro OSL_WRITE_ACTIVE_SLOT_BACKUP
  Goto cleanup

remove_backup:
  Delete "$DOCUMENTS\OSL identity backup.json"

cleanup:
  RMDir /r "$APPDATA\org.oslprivacy.hub"
  RMDir /r "$LOCALAPPDATA\org.oslprivacy.hub"
  RMDir /r "$APPDATA\osl"
!macroend
