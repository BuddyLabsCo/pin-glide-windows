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
!define KEY_WOW64_64KEY 0x100
!define MOVEFILE_REPLACE_EXISTING_WRITE_THROUGH 9

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
Var VersionChanged

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT}"
InstallDir "${INSTALL_DIR}"
VIProductVersion "${FILE_VERSION}"
VIFileVersion "${FILE_VERSION}"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName" "BuddyLabsCo"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Setup"
VIAddVersionKey "LegalCopyright" ""
VIAddVersionKey "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "FileVersion" "${PRODUCT_VERSION}"

!ifdef SIGNTOOL
    !uninstfinalize '"${SIGNTOOL}" ${SIGN_ARGS} "%1"' = 0
    !uninstfinalize '"${SIGNTOOL}" ${VERIFY_ARGS} "%1"' = 0
!endif

Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

!macro Fail message
    MessageBox MB_OK|MB_ICONSTOP "${message}" /SD IDOK
    SetErrorLevel 2
    Abort
!macroend

!macro Initialize prefix
Function ${prefix}.onInit
    SetShellVarContext current
    SetRegView 64
    ${If} "${USER_FILES}" == ""
    ${OrIf} "${MENU_DIR}" == ""
        !insertmacro Fail "Windows could not locate the current user's folders."
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
        !insertmacro Fail "Exit Pin Glide from its tray menu, then try again."
    ${EndIf}
    ${If} $1 != 0
        Return
    ${EndIf}
unavailable:
    !insertmacro Fail "Setup could not check whether Pin Glide is running."
FunctionEnd
!macroend

!insertmacro RequireStopped ""
!insertmacro RequireStopped "un."

!macro Move source destination
    System::Call 'kernel32::MoveFileExW(w "$INSTDIR\${source}", w "$INSTDIR\${destination}", i ${MOVEFILE_REPLACE_EXISTING_WRITE_THROUGH}) i.r0'
!macroend

!macro RegisterUninstaller
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "BuddyLabsCo"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\${UNINSTALL_EXE}"'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
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
        !insertmacro Fail "Pin Glide requires x64 Windows 10 22H2 or Windows 11."
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
        CreateShortCut "${SHORTCUT}" "$INSTDIR\${RECEIVER_EXE}"
        IfErrors failed
    ${EndIf}
    ClearErrors
    ReadRegStr $0 HKCU "${UNINSTALL_KEY}" "UninstallString"
    ClearErrors
    ${If} $0 == ""
        StrCpy $UninstallEntryCreated 1
        !insertmacro RegisterUninstaller
        IfErrors failed
    ${Else}
        ReadRegStr $PreviousVersion HKCU "${UNINSTALL_KEY}" "DisplayVersion"
        ${IfNot} ${Errors}
            StrCpy $HadVersion 1
        ${EndIf}
        ClearErrors
        WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
        IfErrors failed
        StrCpy $VersionChanged 1
    ${EndIf}

    !insertmacro Move "${UNINSTALL_STAGE}" "${UNINSTALL_EXE}"
    StrCmp $0 0 failed
    StrCpy $UninstallerReplaced 1
    !insertmacro Move "${RECEIVER_STAGE}" "${RECEIVER_EXE}"
    StrCmp $0 0 failed
    Delete "$INSTDIR\${UNINSTALL_BACKUP}"
    Goto done

failed:
    ClearErrors
    ${If} $VersionChanged == 1
        ${If} $HadVersion == 1
            WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "$PreviousVersion"
        ${Else}
            DeleteRegValue HKCU "${UNINSTALL_KEY}" "DisplayVersion"
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
        !insertmacro Fail "Pin Glide could not be installed and setup could not restore the previous installation completely."
    ${EndIf}
    !insertmacro Fail "Pin Glide could not be installed. Close programs using the installation files and try again."
done:
SectionEnd

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
    Delete "${SHORTCUT}"
    !insertmacro DeleteStartupValue "${RUN_KEY}"
    !insertmacro DeleteStartupValue "${APPROVAL_KEY}"
    IfErrors failed

    Call un.RemoveEntryAndExecutable
    IfErrors failed
    RMDir "$INSTDIR"
    Goto done

failed:
    !insertmacro Fail "Pin Glide could not be completely removed. Close programs using its files and try again."
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
            ${EndIf}
            SetErrors
        ${EndIf}
    ${EndIf}
    ${If} $1 != -1
        System::Call 'kernel32::CloseHandle(p r1)'
    ${EndIf}
FunctionEnd
