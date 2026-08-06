!define OSL_BACKUP_DIR "$DOCUMENTS\OSL local data backup"
!define OSL_OLD_IDENTITY_BACKUP "$DOCUMENTS\OSL identity backup.json"

!macro OSL_WRITE_ACTIVE_SLOT_IDENTITY_BACKUP
  ClearErrors
  FileOpen $0 "$APPDATA\org.oslprivacy.hub\osl-core\hub-active-identity" r
  IfErrors active_slot_done
  FileRead $0 $1
  FileClose $0
  IfFileExists "$APPDATA\org.oslprivacy.hub\osl-core\hub-identities\$1\identity.json" 0 active_slot_done
    CreateDirectory "${OSL_BACKUP_DIR}\org.oslprivacy.hub\osl-core\hub-identities"
    CreateDirectory "${OSL_BACKUP_DIR}\org.oslprivacy.hub\osl-core\hub-identities\$1"
    CopyFiles /SILENT "$APPDATA\org.oslprivacy.hub\osl-core\hub-identities\$1\identity.json" "${OSL_BACKUP_DIR}\org.oslprivacy.hub\osl-core\hub-identities\$1"
active_slot_done:
!macroend

!macro OSL_WRITE_LOCAL_DATA_BACKUP
  RMDir /r "${OSL_BACKUP_DIR}"
  Delete "${OSL_OLD_IDENTITY_BACKUP}"
  CreateDirectory "${OSL_BACKUP_DIR}"
  CreateDirectory "${OSL_BACKUP_DIR}\org.oslprivacy.hub"
  CreateDirectory "${OSL_BACKUP_DIR}\org.oslprivacy.hub\osl-core"
  CopyFiles /SILENT "$APPDATA\org.oslprivacy.hub\osl-core\hub-active-identity" "${OSL_BACKUP_DIR}\org.oslprivacy.hub\osl-core"
  IfFileExists "$APPDATA\org.oslprivacy.hub\osl-core\identity.json" 0 +3
    CopyFiles /SILENT "$APPDATA\org.oslprivacy.hub\osl-core\identity.json" "${OSL_BACKUP_DIR}\org.oslprivacy.hub\osl-core"
    Goto messages_backup
  !insertmacro OSL_WRITE_ACTIVE_SLOT_IDENTITY_BACKUP
messages_backup:
  IfFileExists "$APPDATA\org.oslprivacy.hub\osl-core\messages\*.*" 0 local_backup_done
    CreateDirectory "${OSL_BACKUP_DIR}\org.oslprivacy.hub\osl-core\messages"
    CopyFiles /SILENT "$APPDATA\org.oslprivacy.hub\osl-core\messages\*.*" "${OSL_BACKUP_DIR}\org.oslprivacy.hub\osl-core\messages"
local_backup_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  IfSilent remove_backup 0
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Write one OSL local data backup to your Documents folder before uninstall removes local OSL data?" \
    IDYES keep_backup IDNO remove_backup

keep_backup:
  !insertmacro OSL_WRITE_LOCAL_DATA_BACKUP
  Goto cleanup

remove_backup:
  RMDir /r "${OSL_BACKUP_DIR}"
  Delete "${OSL_OLD_IDENTITY_BACKUP}"

cleanup:
  RMDir /r "$APPDATA\org.oslprivacy.hub"
  RMDir /r "$LOCALAPPDATA\org.oslprivacy.hub"
  RMDir /r "$APPDATA\osl"
!macroend
