; Bongo Whale installer (NSIS 3.x)
; Build with: makensis installer\bongocat.nsi
; Produces:   dist\BongoWhale-Setup.exe

!include "MUI2.nsh"

!define APPNAME "鲸鱼娘 Bongo Cat"
!define SHORTNAME "BongoWhale"
!define EXE "bongocat.exe"
!define VERSION "0.1.0"

Name "${APPNAME}"
OutFile "..\dist\BongoWhale-Setup.exe"
InstallDir "$LOCALAPPDATA\${SHORTNAME}"
InstallDirRegKey HKCU "Software\${SHORTNAME}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Unicode true

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File "..\target\release\${EXE}"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortCut "$SMPROGRAMS\${APPNAME}.lnk" "$INSTDIR\${EXE}"
  CreateShortCut "$SMPROGRAMS\卸载 ${APPNAME}.lnk" "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "Software\${SHORTNAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SHORTNAME}" "DisplayName" "${APPNAME}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SHORTNAME}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SHORTNAME}" "DisplayIcon" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SHORTNAME}" "Publisher" "Bongo Whale"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SHORTNAME}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SHORTNAME}" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SHORTNAME}" "NoRepair" 1

  Exec '"$INSTDIR\${EXE}"'
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APPNAME}.lnk"
  Delete "$SMPROGRAMS\卸载 ${APPNAME}.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SHORTNAME}"
  DeleteRegKey HKCU "Software\${SHORTNAME}"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${SHORTNAME}"
SectionEnd
