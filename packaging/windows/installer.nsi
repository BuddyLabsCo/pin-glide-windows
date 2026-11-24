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
!define /ifndef RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define /ifndef APPROVAL_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define ERROR_ALREADY_EXISTS 183
!define FILE_ATTRIBUTE_REPARSE_POINT 0x400
!define MOVEFILE_REPLACE_EXISTING_WRITE_THROUGH 9

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT}"
InstallDir "${INSTALL_DIR}"

!ifdef SIGNTOOL
    !uninstfinalize '"${SIGNTOOL}" sign /sha1 ${THUMBPRINT} /fd SHA256 /tr "${TIMESTAMP_URL}" /td SHA256 "%1"' = 0
    !uninstfinalize '"${SIGNTOOL}" verify /pa /tw "%1"' = 0
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

Section
    ${IfNot} ${IsNativeAMD64}
    ${OrIfNot} ${AtLeastWin10}
    ${OrIfNot} ${AtLeastBuild} 19045
        !insertmacro Fail "Pin Glide requires x64 Windows 10 22H2 or Windows 11."
    ${EndIf}
    Call RequireStopped
    StrCpy $R0 0
    StrCpy $R1 0
    IfFileExists "$INSTDIR\${RECEIVER_EXE}" 0 +2
        StrCpy $R0 1

    ClearErrors
    SetOutPath "$INSTDIR"
    IfErrors failed
    SetOverwrite try
    File /oname=receiver.new "${RECEIVER}"
    IfErrors failed
    WriteUninstaller "$INSTDIR\uninstall.new"
    IfErrors failed

    ${If} $R0 == 0
        CreateShortCut "${SHORTCUT}" "$INSTDIR\${RECEIVER_EXE}"
        WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
        WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "BuddyLabsCo"
        WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\${UNINSTALL_EXE}"'
        WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
        WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
        IfErrors failed
        ${IfNot} ${FileExists} "${DATA_DIR}\settings.json"
            ReadRegStr $0 HKCU "${RUN_KEY}" "${PRODUCT_NAME}"
            ${If} ${Errors}
                ClearErrors
                WriteRegStr HKCU "${RUN_KEY}" "${PRODUCT_NAME}" '"$INSTDIR\${RECEIVER_EXE}" --startup'
                IfErrors failed
                StrCpy $R1 1
            ${EndIf}
        ${EndIf}
    ${EndIf}

    System::Call 'kernel32::MoveFileExW(w "$INSTDIR\uninstall.new", w "$INSTDIR\${UNINSTALL_EXE}", i ${MOVEFILE_REPLACE_EXISTING_WRITE_THROUGH}) i.r0'
    StrCmp $0 0 failed
    System::Call 'kernel32::MoveFileExW(w "$INSTDIR\receiver.new", w "$INSTDIR\${RECEIVER_EXE}", i ${MOVEFILE_REPLACE_EXISTING_WRITE_THROUGH}) i.r0'
    StrCmp $0 0 failed
    Goto done

failed:
    Delete "$INSTDIR\receiver.new"
    Delete "$INSTDIR\uninstall.new"
    ${If} $R0 == 0
        ${If} $R1 != 0
            DeleteRegValue HKCU "${RUN_KEY}" "${PRODUCT_NAME}"
        ${EndIf}
        DeleteRegKey HKCU "${UNINSTALL_KEY}"
        Delete "${SHORTCUT}"
        Delete "$INSTDIR\${UNINSTALL_EXE}"
        SetOutPath "${USER_FILES}"
        RMDir "$INSTDIR"
    ${EndIf}
    !insertmacro Fail "Pin Glide could not be installed. Close programs using the installation files and try again."
done:
SectionEnd

!macro RemoveStartupValue key
    System::Call 'advapi32::RegDeleteKeyValueW(p 0x80000001, w "${key}", w "${PRODUCT_NAME}") i.r0'
    ${If} $0 != 0
    ${AndIf} $0 != 2
        SetErrors
    ${EndIf}
!macroend

Section "Uninstall"
    Call un.RequireStopped
    System::Call 'kernel32::GetFileAttributesW(w "${DATA_DIR}") i.r0 ?e'
    Pop $1
    ${If} $0 == -1
    ${AndIf} $1 != 2
    ${AndIf} $1 != 3
        Goto failed
    ${EndIf}
    ${If} $0 != -1
        IntOp $0 $0 & ${FILE_ATTRIBUTE_REPARSE_POINT}
        ${If} $0 != 0
            Goto failed
        ${EndIf}
    ${EndIf}

    ClearErrors
    Delete "$INSTDIR\${RECEIVER_EXE}"
    IfErrors failed
    Delete "${DATA_DIR}\settings.json"
    Delete "${DATA_DIR}\registration.json"
    Delete "${DATA_DIR}\registration.pending.json"
    Delete "${DATA_DIR}\registration.cleanup.json"
    Delete "${DATA_DIR}\delivery-rejections.log"
    Delete "${DATA_DIR}\.tmp??????"
    Delete "${SHORTCUT}"
    !insertmacro RemoveStartupValue "${RUN_KEY}"
    !insertmacro RemoveStartupValue "${APPROVAL_KEY}"
    IfErrors failed

    RMDir "${DATA_DIR}"
    ClearErrors
    Delete "$INSTDIR\${UNINSTALL_EXE}"
    IfErrors failed
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
    IfErrors failed
    RMDir "$INSTDIR"
    Goto done

failed:
    !insertmacro Fail "Pin Glide could not be completely removed. Close programs using its files and try again."
done:
SectionEnd
