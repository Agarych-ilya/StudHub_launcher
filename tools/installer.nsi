Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "nsDialogs.nsh"
Var CreateDesktop
Var DesktopCheckbox
!ifndef APP_EXE
  !error "Pass /DAPP_EXE=/absolute/path/studhub-launcher.exe"
!endif
!ifndef WEBVIEW_EXE
  !error "Pass /DWEBVIEW_EXE=/absolute/path/MicrosoftEdgeWebview2Setup.exe"
!endif
!ifndef OUTPUT_FILE
  !define OUTPUT_FILE "StudHubLauncher-0.4.8-Setup.exe"
!endif
Name "StudHub Launcher"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\StudHub Launcher"
InstallDirRegKey HKCU "Software\StudHub\Launcher" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
VIProductVersion "0.4.8.0"
VIAddVersionKey /LANG=1033 "ProductName" "StudHub Launcher"
VIAddVersionKey /LANG=1033 "FileDescription" "StudHub Launcher installer"
VIAddVersionKey /LANG=1033 "FileVersion" "0.4.8"
VIAddVersionKey /LANG=1033 "LegalCopyright" "StudHub"
!define MUI_ABORTWARNING
!define MUI_ICON "../src-tauri/icons/icon.ico"
!define MUI_UNICON "../src-tauri/icons/icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\StudHubLauncher.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Запустить StudHub Launcher"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
Page custom ShortcutPage ShortcutLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "English"
Function .onInit
  StrCpy $CreateDesktop 1
  ReadRegStr $0 HKCU "Software\StudHub\Launcher" "DesktopShortcut"
  ${If} $0 == "0"
    StrCpy $CreateDesktop 0
  ${EndIf}
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "StudHub Launcher требует 64-битную Windows 10/11."
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_ICONSTOP "Требуется Windows 10 или новее."
    Abort
  ${EndIf}
  FindWindow $0 "" "StudHub Launcher"
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Закройте StudHub Launcher перед установкой."
    Abort
  ${EndIf}
FunctionEnd
Function ShortcutPage
  !insertmacro MUI_HEADER_TEXT "Ярлык лаунчера" "Выберите, нужен ли ярлык на рабочем столе."
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}
  ${NSD_CreateCheckbox} 0 20u 100% 24u "Создать ярлык StudHub Launcher на рабочем столе"
  Pop $DesktopCheckbox
  ${NSD_SetState} $DesktopCheckbox $CreateDesktop
  nsDialogs::Show
FunctionEnd
Function ShortcutLeave
  ${NSD_GetState} $DesktopCheckbox $CreateDesktop
FunctionEnd
Section "StudHub Launcher"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File /oname=StudHubLauncher.exe "${APP_EXE}"
  !if /FileExists "../deployment.json"
    File /oname=deployment.json "../deployment.json"
  !endif
  SetOutPath "$INSTDIR\resources"
  File /oname=studhub-pack-1.0.2.zip "../src-tauri/resources/studhub-pack-1.0.2.zip"
  SetOutPath "$INSTDIR"
  ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 == ""
    SetRegView 32
    ReadRegStr $0 HKLM "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${EndIf}
  ${If} $0 == ""
  ${OrIf} $0 == "0.0.0.0"
    InitPluginsDir
    File /oname=$PLUGINSDIR\MicrosoftEdgeWebview2Setup.exe "${WEBVIEW_EXE}"
    DetailPrint "Подготовка Microsoft WebView2 (нужно подключение к Интернету)..."
    ExecWait '"$PLUGINSDIR\MicrosoftEdgeWebview2Setup.exe" /silent /install' $1
    ${If} $1 != 0
    ${AndIf} $1 != 3010
      MessageBox MB_ICONEXCLAMATION "WebView2 не установлен (код $1). Установите Microsoft WebView2 Runtime и повторите запуск."
    ${EndIf}
  ${EndIf}
  WriteRegStr HKCU "Software\StudHub\Launcher" "InstallDir" "$INSTDIR"
  CreateDirectory "$SMPROGRAMS\StudHub Launcher"
  CreateShortcut "$SMPROGRAMS\StudHub Launcher\StudHub Launcher.lnk" "$INSTDIR\StudHubLauncher.exe"
  Delete "$SMPROGRAMS\StudHub Launcher\Инструкция.lnk"
  Delete "$INSTDIR\Instruction.txt"
  ${If} $CreateDesktop == 1
    CreateShortcut "$DESKTOP\StudHub Launcher.lnk" "$INSTDIR\StudHubLauncher.exe"
  ${Else}
    Delete "$DESKTOP\StudHub Launcher.lnk"
  ${EndIf}
  WriteRegStr HKCU "Software\StudHub\Launcher" "DesktopShortcut" "$CreateDesktop"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher" "DisplayName" "StudHub Launcher"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher" "DisplayVersion" "0.4.8"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher" "Publisher" "StudHub"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher" "DisplayIcon" "$INSTDIR\StudHubLauncher.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher" "NoRepair" 1
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  FindWindow $0 "" "StudHub Launcher"
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Закройте StudHub Launcher перед удалением."
    Abort
  ${EndIf}
  Delete "$INSTDIR\StudHubLauncher.exe"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$INSTDIR\Instruction.txt"
  Delete "$INSTDIR\resources\studhub-pack-1.0.2.zip"
  RMDir "$INSTDIR\resources"
  Delete "$DESKTOP\StudHub Launcher.lnk"
  Delete "$SMPROGRAMS\StudHub Launcher\StudHub Launcher.lnk"
  Delete "$SMPROGRAMS\StudHub Launcher\Инструкция.lnk"
  RMDir "$SMPROGRAMS\StudHub Launcher"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\StudHub\Launcher"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\StudHubLauncher"
  ; Minecraft, mods, settings and credentials remain in the user's data directories.
SectionEnd
