LoadLanguageFile "${NSISDIR}\Contrib\Language files\English.nlf"
LoadLanguageFile "${NSISDIR}\Contrib\Language files\SimpChinese.nlf"

LangString ProductName ${LANG_ENGLISH} "Pin Glide"
LangString ProductName ${LANG_SIMPCHINESE} "鹊信"
LangString MissingFolders ${LANG_ENGLISH} "Windows could not locate the current user's folders."
LangString MissingFolders ${LANG_SIMPCHINESE} "Windows 无法找到当前用户的文件夹。"
LangString ReceiverRunning ${LANG_ENGLISH} "Exit Pin Glide from its tray menu, then try again."
LangString ReceiverRunning ${LANG_SIMPCHINESE} "请从系统托盘菜单退出鹊信，然后重试。"
LangString InstanceUnavailable ${LANG_ENGLISH} "Could not check whether Pin Glide is running."
LangString InstanceUnavailable ${LANG_SIMPCHINESE} "无法检查鹊信是否正在运行。"
LangString UnsupportedWindows ${LANG_ENGLISH} "Pin Glide requires x64 Windows 10 22H2 or Windows 11."
LangString UnsupportedWindows ${LANG_SIMPCHINESE} "鹊信需要 x64 版本的 Windows 10 22H2 或 Windows 11。"
LangString RollbackFailure ${LANG_ENGLISH} "Pin Glide could not be installed and setup could not restore the previous installation completely."
LangString RollbackFailure ${LANG_SIMPCHINESE} "无法安装鹊信，且安装程序未能完全恢复之前的安装。"
LangString InstallFailure ${LANG_ENGLISH} "Pin Glide could not be installed. Close programs using the installation files and try again."
LangString InstallFailure ${LANG_SIMPCHINESE} "无法安装鹊信。请关闭正在使用安装文件的程序，然后重试。"
LangString UninstallFailure ${LANG_ENGLISH} "Pin Glide could not be completely removed. Close programs using its files and try again."
LangString UninstallFailure ${LANG_SIMPCHINESE} "无法完全卸载鹊信。请关闭正在使用其文件的程序，然后重试。"

!macro LocalizedVersionKeys
    !insertmacro VersionKeys ${LANG_SIMPCHINESE} "鹊信" "鹊信安装程序"
!macroend

!macro SelectLanguage
    !ifdef TEST_UI_LANGUAGE
        StrCpy $0 ${TEST_UI_LANGUAGE}
    !else
        System::Call 'kernel32::GetUserDefaultUILanguage() i.r0'
    !endif
    IntOp $0 $0 & 0x03ff
    ${If} $0 == 4
        StrCpy $LANGUAGE ${LANG_SIMPCHINESE}
    ${Else}
        StrCpy $LANGUAGE ${LANG_ENGLISH}
    ${EndIf}
    !ifdef TEST_UI_LANGUAGE
        FileOpen $0 "${USER_FILES}\language.txt" w
        FileWrite $0 "$LANGUAGE"
        FileClose $0
    !endif
!macroend
