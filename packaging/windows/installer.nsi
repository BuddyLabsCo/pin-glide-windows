Unicode true
RequestExecutionLevel user
CRCCheck force

!include "LogicLib.nsh"
!include "WinVer.nsh"
!include "x64.nsh"

!define /ifndef PRODUCT_NAME "Pin Glide"
!define /ifndef USER_FILES "$LOCALAPPDATA"
!define /ifndef MENU_DIR "$SMPROGRAMS"
!define INSTALL_DIR "${USER_FILES}\Programs\${PRODUCT_NAME}"
!define DATA_DIR "${USER_FILES}\${PRODUCT_NAME}"
!define SHORTCUT "${MENU_DIR}\${PRODUCT_NAME}.lnk"
!define RECEIVER_EXE "windows-app.exe"
!define UNINSTALL_EXE "Uninstall.exe"
!define RECEIVER_STAGE "receiver.new"
!define UNINSTALL_STAGE "uninstall.new"
!define UNINSTALL_BACKUP "uninstall.previous"
!define SHORTCUT_BACKUP "shortcut.previous"
!define /ifndef RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define /ifndef APPROVAL_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define HKCU_HANDLE 0x80000001
!define ERROR_FILE_NOT_FOUND 2
!define ERROR_PATH_NOT_FOUND 3
!define ERROR_ALREADY_EXISTS 183
!define FILE_ATTRIBUTE_REPARSE_POINT 0x400
!define DELETE_ACCESS 0x10000
!define OPEN_EXISTING 3
!define FILE_DISPOSITION_INFO 4
!define RRF_RT_REG_SZ 2
!define RRF_REG_SZ_64KEY_NOEXPAND 0x10010002
!define RRF_REG_DWORD_64KEY 0x10010
!define KEY_WOW64_64KEY 0x100
!define KEY_QUERY_VALUE_64KEY 0x101
!define MOVEFILE_REPLACE_EXISTING_WRITE_THROUGH 9
!define BRAND_ICON "..\..\crates\windows-app\ui\assets\app-icon.ico"
!define PRODUCT_RESOURCE 101

Var HasReceiver
Var HasUninstaller
Var UninstallerReplaced
Var PreviousRun
Var HadRunValue
Var RunChanged
Var RollbackFailed
Var ShortcutCreated
Var UninstallEntryCreated
Var PreviousVersion
Var HadVersion
Var PreviousName
Var HadName
Var PreviousLocalizedName
Var HadLocalizedName
Var PreviousIcon
Var HadIcon
Var MetadataChanged
Var PreviousCommand
Var HadCommand
Var PreviousPublisher
Var HadPublisher
Var PreviousNoModify
Var HadNoModify
Var PreviousNoRepair
Var HadNoRepair
Var IncompleteEntryChanged
Var ShortcutLocalized
Var PreviousShortcutModule
Var PreviousShortcutId
Var ShortcutBackedUp

!include /CHARSET=UTF8 "installer-language.nsh"
Name "$(ProductName)"
OutFile "${OUTPUT}"
InstallDir "${INSTALL_DIR}"
Icon "${BRAND_ICON}"
UninstallIcon "${BRAND_ICON}"
VIProductVersion "${FILE_VERSION}"
VIFileVersion "${FILE_VERSION}"
!macro VersionKeys language name description
    VIAddVersionKey /LANG=${language} "ProductName" "${name}"
    VIAddVersionKey /LANG=${language} "CompanyName" "BuddyLabsCo"
    VIAddVersionKey /LANG=${language} "FileDescription" "${description}"
    VIAddVersionKey /LANG=${language} "LegalCopyright" ""
    VIAddVersionKey /LANG=${language} "ProductVersion" "${PRODUCT_VERSION}"
    VIAddVersionKey /LANG=${language} "FileVersion" "${PRODUCT_VERSION}"
!macroend
!insertmacro VersionKeys ${LANG_ENGLISH} "Pin Glide" "Pin Glide Setup"
!insertmacro LocalizedVersionKeys

!uninstfinalize 'powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "resource-check.ps1" -Executable "%1" -Kind Uninstaller' = 0
!ifdef SIGNTOOL
    !uninstfinalize '"${SIGNTOOL}" ${SIGN_ARGS} "%1"' = 0
    !uninstfinalize '"${SIGNTOOL}" ${VERIFY_ARGS} "%1"' = 0
!endif

Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

!macro Fail message
    !ifdef TEST_UI_LANGUAGE
        FileOpen $9 "${USER_FILES}\failure.txt" w
        FileWriteUTF16LE $9 "${message}"
        FileClose $9
    !endif
    MessageBox MB_OK|MB_ICONSTOP "${message}" /SD IDOK
    SetErrorLevel 2
    Abort
!macroend

!macro Initialize prefix
Function ${prefix}.onInit
    !insertmacro SelectLanguage
    SetShellVarContext current
    SetRegView 64
    ${If} "${USER_FILES}" == ""
    ${OrIf} "${MENU_DIR}" == ""
        !insertmacro Fail "$(MissingFolders)"
    ${EndIf}
    StrCpy $INSTDIR "${INSTALL_DIR}"
FunctionEnd
!macroend

!insertmacro Initialize ""
!insertmacro Initialize "un"

!macro RequireStopped prefix
Function ${prefix}RequireStopped
    System::Call 'advapi32::GetTokenInformation(p -4, i 1, p 0, i 0, *i .r0)'
    ${If} $0 == 0
        Goto unavailable
    ${EndIf}
    System::Alloc $0
    Pop $1
    System::Call 'advapi32::GetTokenInformation(p -4, i 1, p r1, i r0, *i .r0) i.r2'
    ${If} $2 != 0
        System::Call '*$1(p .r2)'
        System::Call 'advapi32::ConvertSidToStringSidW(p r2, *p .r3) i.r2'
    ${EndIf}
    System::Free $1
    ${If} $2 == 0
        Goto unavailable
    ${EndIf}
    System::Call '*$3(&w${NSIS_MAX_STRLEN} .r0)'
    System::Call 'kernel32::LocalFree(p r3)'
    ; Hold the Receiver's per-user instance handle until setup exits.
    System::Call 'kernel32::CreateMutexW(p 0, i 0, w "Global\BuddyLabsCo.PinGlide.$0.Instance") p.r1 ?e'
    Pop $2
    ${If} $2 == ${ERROR_ALREADY_EXISTS}
        !insertmacro Fail "$(ReceiverRunning)"
    ${EndIf}
    ${If} $1 != 0
        Return
    ${EndIf}
unavailable:
    !insertmacro Fail "$(InstanceUnavailable)"
FunctionEnd
!macroend

!insertmacro RequireStopped ""
!insertmacro RequireStopped "un."

!macro Move source destination
    System::Call 'kernel32::MoveFileExW(w "$INSTDIR\${source}", w "$INSTDIR\${destination}", i ${MOVEFILE_REPLACE_EXISTING_WRITE_THROUGH}) i.r0'
!macroend

!macro RegisterUninstaller
    !insertmacro WriteDisplayMetadata
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "BuddyLabsCo"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\${UNINSTALL_EXE}"'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
!macroend

!macro WriteDisplayMetadata
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "$(ProductName)"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName_Localized" "@$INSTDIR\${RECEIVER_EXE},-${PRODUCT_RESOURCE}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${RECEIVER_EXE},0"
!macroend

!macro ReadMetadata value previous had
    IntOp $1 ${NSIS_MAX_STRLEN} * 2
    System::Call 'advapi32::RegGetValueW(p ${HKCU_HANDLE}, w "${UNINSTALL_KEY}", w "${value}", i ${RRF_REG_SZ_64KEY_NOEXPAND}, p 0, w .r2, *i r1) i.r0'
    ${If} $0 == 0
        StrCpy ${previous} $2
        StrCpy ${had} 1
    ${ElseIf} $0 != ${ERROR_FILE_NOT_FOUND}
    ${AndIf} $0 != ${ERROR_PATH_NOT_FOUND}
        Goto failed
    ${EndIf}
!macroend

!macro RestoreMetadata value previous had
    ${If} ${had} == 1
        WriteRegStr HKCU "${UNINSTALL_KEY}" "${value}" "${previous}"
    ${Else}
        DeleteRegValue HKCU "${UNINSTALL_KEY}" "${value}"
    ${EndIf}
!macroend

!macro ReadDwordMetadata value previous had
    System::Call 'advapi32::RegGetValueW(p ${HKCU_HANDLE}, w "${UNINSTALL_KEY}", w "${value}", i ${RRF_REG_DWORD_64KEY}, p 0, *i .r2, *i 4) i.r0'
    ${If} $0 == 0
        StrCpy ${previous} $2
        StrCpy ${had} 1
    ${ElseIf} $0 != ${ERROR_FILE_NOT_FOUND}
    ${AndIf} $0 != ${ERROR_PATH_NOT_FOUND}
        Goto failed
    ${EndIf}
!macroend

!macro RestoreDwordMetadata value previous had
    ${If} ${had} == 1
        WriteRegDWORD HKCU "${UNINSTALL_KEY}" "${value}" "${previous}"
    ${Else}
        DeleteRegValue HKCU "${UNINSTALL_KEY}" "${value}"
    ${EndIf}
!macroend

!macro DeleteStartupValue key
    System::Call 'advapi32::RegDeleteKeyValueW(p ${HKCU_HANDLE}, w "${key}", w "${PRODUCT_NAME}") i.r0'
    ${If} $0 != 0
    ${AndIf} $0 != ${ERROR_FILE_NOT_FOUND}
    ${AndIf} $0 != ${ERROR_PATH_NOT_FOUND}
        SetErrors
    ${EndIf}
!macroend

Section
    ${IfNot} ${IsNativeAMD64}
    ${OrIfNot} ${AtLeastWin10}
    ${OrIfNot} ${AtLeastBuild} 19045
        !insertmacro Fail "$(UnsupportedWindows)"
    ${EndIf}
    Call RequireStopped
    ${If} ${FileExists} "$INSTDIR\${RECEIVER_EXE}"
        StrCpy $HasReceiver 1
    ${EndIf}
    ${If} ${FileExists} "$INSTDIR\${UNINSTALL_EXE}"
        StrCpy $HasUninstaller 1
    ${EndIf}

    ClearErrors
    SetOutPath "$INSTDIR"
    IfErrors failed
    SetOverwrite try
    File /oname=${RECEIVER_STAGE} "${RECEIVER}"
    IfErrors failed
    WriteUninstaller "$INSTDIR\${UNINSTALL_STAGE}"
    IfErrors failed
    ${If} $HasUninstaller == 1
        System::Call 'kernel32::CopyFileW(w "$INSTDIR\${UNINSTALL_EXE}", w "$INSTDIR\${UNINSTALL_BACKUP}", i 0) i.r0'
        StrCmp $0 0 failed
    ${EndIf}

    Call UpdateStartup
    IfErrors failed

    ${IfNot} ${FileExists} "${SHORTCUT}"
        StrCpy $ShortcutCreated 1
    ${Else}
        System::Call 'kernel32::CopyFileW(w "${SHORTCUT}", w "$INSTDIR\${SHORTCUT_BACKUP}", i 0) i.r0'
        StrCmp $0 0 failed
        StrCpy $ShortcutBackedUp 1
    ${EndIf}
    System::Call 'shell32::SHGetLocalizedName(w "${SHORTCUT}", w .r2, i ${NSIS_MAX_STRLEN}, *i .r3) i.r0'
    ${If} $0 == 0
        StrCpy $PreviousShortcutModule $2
        StrCpy $PreviousShortcutId $3
    ${EndIf}
    CreateShortCut "${SHORTCUT}" "$INSTDIR\${RECEIVER_EXE}" "" "$INSTDIR\${RECEIVER_EXE}" 0
    IfErrors failed
    Call SetShortcutIdentity
    IfErrors failed
    System::Call 'shell32::SHSetLocalizedName(w "${SHORTCUT}", w "$INSTDIR\${RECEIVER_EXE}", i ${PRODUCT_RESOURCE}) i.r0'
    StrCmp $0 0 0 failed
    StrCpy $ShortcutLocalized 1
    System::Call 'advapi32::RegOpenKeyExW(p ${HKCU_HANDLE}, w "${UNINSTALL_KEY}", i 0, i ${KEY_QUERY_VALUE_64KEY}, *p .r1) i.r0'
    ${If} $0 == ${ERROR_FILE_NOT_FOUND}
    ${OrIf} $0 == ${ERROR_PATH_NOT_FOUND}
        StrCpy $UninstallEntryCreated 1
        !insertmacro RegisterUninstaller
        IfErrors failed
    ${ElseIf} $0 != 0
        Goto failed
    ${Else}
        System::Call 'advapi32::RegCloseKey(p r1)'
        !insertmacro ReadMetadata "UninstallString" $PreviousCommand $HadCommand
        !insertmacro ReadMetadata "DisplayVersion" $PreviousVersion $HadVersion
        !insertmacro ReadMetadata "DisplayName" $PreviousName $HadName
        !insertmacro ReadMetadata "DisplayName_Localized" $PreviousLocalizedName $HadLocalizedName
        !insertmacro ReadMetadata "DisplayIcon" $PreviousIcon $HadIcon
        ${If} $PreviousCommand == ""
            !insertmacro ReadMetadata "Publisher" $PreviousPublisher $HadPublisher
            !insertmacro ReadDwordMetadata "NoModify" $PreviousNoModify $HadNoModify
            !insertmacro ReadDwordMetadata "NoRepair" $PreviousNoRepair $HadNoRepair
            ClearErrors
            StrCpy $IncompleteEntryChanged 1
            WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "BuddyLabsCo"
            WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\${UNINSTALL_EXE}"'
            WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
            WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
            IfErrors failed
        ${EndIf}
        ClearErrors
        StrCpy $MetadataChanged 1
        !insertmacro WriteDisplayMetadata
        IfErrors failed
        WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
        IfErrors failed
    ${EndIf}

    !insertmacro Move "${UNINSTALL_STAGE}" "${UNINSTALL_EXE}"
    StrCmp $0 0 failed
    StrCpy $UninstallerReplaced 1
    !insertmacro Move "${RECEIVER_STAGE}" "${RECEIVER_EXE}"
    StrCmp $0 0 failed
    Delete "$INSTDIR\${UNINSTALL_BACKUP}"
    Delete "$INSTDIR\${SHORTCUT_BACKUP}"
    Goto done

failed:
    ClearErrors
    ${If} $MetadataChanged == 1
        !insertmacro RestoreMetadata "DisplayName" $PreviousName $HadName
        !insertmacro RestoreMetadata "DisplayName_Localized" $PreviousLocalizedName $HadLocalizedName
        !insertmacro RestoreMetadata "DisplayIcon" $PreviousIcon $HadIcon
        !insertmacro RestoreMetadata "DisplayVersion" $PreviousVersion $HadVersion
    ${EndIf}
    ${If} $IncompleteEntryChanged == 1
        !insertmacro RestoreMetadata "UninstallString" $PreviousCommand $HadCommand
        !insertmacro RestoreMetadata "Publisher" $PreviousPublisher $HadPublisher
        !insertmacro RestoreDwordMetadata "NoModify" $PreviousNoModify $HadNoModify
        !insertmacro RestoreDwordMetadata "NoRepair" $PreviousNoRepair $HadNoRepair
    ${EndIf}
    ${If} $ShortcutLocalized == 1
        ${If} $PreviousShortcutModule != ""
            System::Call 'shell32::SHSetLocalizedName(w "${SHORTCUT}", w "$PreviousShortcutModule", i $PreviousShortcutId) i.r0'
        ${Else}
            System::Call 'shell32::SHRemoveLocalizedName(w "${SHORTCUT}") i.r0'
        ${EndIf}
        ${If} $0 != 0
            StrCpy $RollbackFailed 1
        ${EndIf}
    ${EndIf}
    ${If} $ShortcutBackedUp == 1
        System::Call 'kernel32::CopyFileW(w "$INSTDIR\${SHORTCUT_BACKUP}", w "${SHORTCUT}", i 0) i.r0'
        ${If} $0 == 0
            StrCpy $RollbackFailed 1
        ${EndIf}
    ${EndIf}
    ${If} $RunChanged == 1
        ${If} $HadRunValue == 1
            WriteRegStr HKCU "${RUN_KEY}" "${PRODUCT_NAME}" "$PreviousRun"
        ${Else}
            !insertmacro DeleteStartupValue "${RUN_KEY}"
        ${EndIf}
    ${EndIf}
    ${If} $UninstallerReplaced == 1
        ${If} $HasUninstaller == 1
            !insertmacro Move "${UNINSTALL_BACKUP}" "${UNINSTALL_EXE}"
            ${If} $0 == 0
                StrCpy $RollbackFailed 1
            ${EndIf}
        ${Else}
            Delete "$INSTDIR\${UNINSTALL_EXE}"
        ${EndIf}
    ${EndIf}
    ${If} ${Errors}
        StrCpy $RollbackFailed 1
    ${EndIf}
    Delete "$INSTDIR\${RECEIVER_STAGE}"
    Delete "$INSTDIR\${UNINSTALL_STAGE}"
    ${If} $RollbackFailed != 1
        Delete "$INSTDIR\${UNINSTALL_BACKUP}"
        Delete "$INSTDIR\${SHORTCUT_BACKUP}"
    ${EndIf}
    ${If} $UninstallEntryCreated == 1
        DeleteRegKey HKCU "${UNINSTALL_KEY}"
    ${EndIf}
    ${If} $ShortcutCreated == 1
        Delete "${SHORTCUT}"
    ${EndIf}
    ${If} $HasReceiver != 1
    ${AndIf} $HasUninstaller != 1
        SetOutPath "${USER_FILES}"
        RMDir "$INSTDIR"
    ${EndIf}
    ${If} $RollbackFailed == 1
        !insertmacro Fail "$(RollbackFailure)"
    ${EndIf}
    !insertmacro Fail "$(InstallFailure)"
done:
SectionEnd

Function SetShortcutIdentity
    System::Call 'shell32::SHGetPropertyStoreFromParsingName(w "${SHORTCUT}", p 0, i 2, g "{886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99}", *p .r1) i.r0'
    ${If} $0 != 0
        SetErrors
        Return
    ${EndIf}
    System::Call '*(&g16 "{9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}", i 5) p.r2'
    System::Call '*(&w32 "BuddyLabsCo.PinGlide") p.r3'
    System::Call '*(i 31, i 0, p r3, i 0) p.r4'
    System::Call '$1->6(p r2, p r4) i.r0'
    ${If} $0 == 0
        System::Call '$1->7() i.r0'
    ${EndIf}
    System::Call '$1->2()'
    System::Free $4
    System::Free $3
    System::Free $2
    ${If} $0 != 0
        SetErrors
    ${EndIf}
FunctionEnd

Function UpdateStartup
    IntOp $1 ${NSIS_MAX_STRLEN} * 2
    System::Call 'advapi32::RegGetValueW(p ${HKCU_HANDLE}, w "${RUN_KEY}", w "${PRODUCT_NAME}", i ${RRF_RT_REG_SZ}, p 0, w .r2, *i r1) i.r0'
    ${If} $0 == 0
        StrCpy $PreviousRun $2
        StrCpy $HadRunValue 1
    ${ElseIf} $0 != ${ERROR_FILE_NOT_FOUND}
    ${AndIf} $0 != ${ERROR_PATH_NOT_FOUND}
        SetErrors
        Return
    ${ElseIf} $HasReceiver == 1
    ${OrIf} $HasUninstaller == 1
    ${OrIf} ${FileExists} "${DATA_DIR}\settings.json"
        Return
    ${EndIf}
    StrCpy $0 '"$INSTDIR\${RECEIVER_EXE}" --startup'
    StrCmpS $0 $PreviousRun done
    WriteRegStr HKCU "${RUN_KEY}" "${PRODUCT_NAME}" "$0"
    ${If} ${Errors}
        SetErrors
        Return
    ${EndIf}
    StrCpy $RunChanged 1
done:
FunctionEnd

Section "Uninstall"
    Call un.RequireStopped
    ${If} ${FileExists} "${SHORTCUT}"
        ; The Shell returns S_OK when the existing shortcut has no localized name.
        System::Call 'shell32::SHRemoveLocalizedName(w "${SHORTCUT}") i.r0'
        StrCmp $0 0 0 failed
        ClearErrors
        Delete "${SHORTCUT}"
        IfErrors failed
    ${EndIf}
    System::Call 'kernel32::GetFileAttributesW(w "${DATA_DIR}") i.r0 ?e'
    Pop $1
    ${If} $0 == -1
    ${AndIf} $1 != ${ERROR_FILE_NOT_FOUND}
    ${AndIf} $1 != ${ERROR_PATH_NOT_FOUND}
        Goto failed
    ${EndIf}
    ClearErrors
    ${If} $0 != -1
        IntOp $0 $0 & ${FILE_ATTRIBUTE_REPARSE_POINT}
        ${If} $0 != 0
            Goto failed
        ${EndIf}
        ; The Receiver owns this fixed, flat data directory.
        Delete "${DATA_DIR}\*.*"
        RMDir "${DATA_DIR}"
        IfErrors failed
    ${EndIf}

    Delete "$INSTDIR\${RECEIVER_EXE}"
    Delete "$INSTDIR\${RECEIVER_STAGE}"
    Delete "$INSTDIR\${UNINSTALL_STAGE}"
    Delete "$INSTDIR\${UNINSTALL_BACKUP}"
    Delete "$INSTDIR\${SHORTCUT_BACKUP}"
    !insertmacro DeleteStartupValue "${RUN_KEY}"
    !insertmacro DeleteStartupValue "${APPROVAL_KEY}"
    IfErrors failed

    Call un.RemoveEntryAndExecutable
    IfErrors failed
    RMDir "$INSTDIR"
    Goto done

failed:
    !insertmacro Fail "$(UninstallFailure)"
done:
SectionEnd

Function un.RemoveEntryAndExecutable
    ; Hold exclusive delete access while unregistering so a locked file leaves a retry command.
    System::Call 'kernel32::CreateFileW(w "$INSTDIR\${UNINSTALL_EXE}", i ${DELETE_ACCESS}, i 0, p 0, i ${OPEN_EXISTING}, i 0, p 0) p.r1 ?e'
    Pop $2
    ${If} $1 == -1
    ${AndIf} $2 != ${ERROR_FILE_NOT_FOUND}
    ${AndIf} $2 != ${ERROR_PATH_NOT_FOUND}
        SetErrors
        Return
    ${EndIf}

    ReadRegStr $PreviousName HKCU "${UNINSTALL_KEY}" "DisplayName"
    ClearErrors
    System::Call 'advapi32::RegDeleteKeyExW(p ${HKCU_HANDLE}, w "${UNINSTALL_KEY}", i ${KEY_WOW64_64KEY}, i 0) i.r2'
    ${If} $2 != 0
    ${AndIf} $2 != ${ERROR_FILE_NOT_FOUND}
    ${AndIf} $2 != ${ERROR_PATH_NOT_FOUND}
        SetErrors
    ${ElseIf} $1 != -1
        System::Call 'kernel32::SetFileInformationByHandle(p r1, i ${FILE_DISPOSITION_INFO}, *i 1, i 1) i.r0'
        ${If} $0 == 0
            ${If} $2 == 0
                !insertmacro RegisterUninstaller
                ${If} $PreviousName != ""
                    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "$PreviousName"
                ${EndIf}
            ${EndIf}
            SetErrors
        ${EndIf}
    ${EndIf}
    ${If} $1 != -1
        System::Call 'kernel32::CloseHandle(p r1)'
    ${EndIf}
FunctionEnd
