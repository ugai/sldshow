!include "MUI2.nsh"
!include "FileFunc.nsh"

Unicode True
SetCompressor /SOLID lzma
RequestExecutionLevel user

!getdllversion "..\target\release\sldshow.exe" Version_
!define VER_MAJOR ${Version_1}
!define VER_MINOR ${Version_2}
!define VER_PATCH ${Version_3}

!define SLDSHOW_NAME "sldshow"
!define SLDSHOW_SEMANTIC_VERSION "${VER_MAJOR}.${VER_MINOR}.${VER_PATCH}"
!define SLDSHOW_PRODUCT_VERSION "${VER_MAJOR}.${VER_MINOR}.${VER_PATCH}.0"
!define SLDSHOW_HOMEPAGE "https://github.com/ugai/sldshow/"
!define SLDSHOW_REG_KEY "Software\sldshow"

!define SLDSHOW_UNINST_EXE "uninstall.exe"
!define SLDSHOW_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\sldshow"

InstallDir "$LOCALAPPDATA\Programs\${SLDSHOW_NAME}"
InstallDirRegKey HKCU "${SLDSHOW_REG_KEY}" ""

Name "${SLDSHOW_NAME}"
Outfile "sldshow-${SLDSHOW_SEMANTIC_VERSION}-setup.exe"

VIProductVersion "${SLDSHOW_PRODUCT_VERSION}"
VIAddVersionKey "ProductName" "${SLDSHOW_NAME}"
VIAddVersionKey "ProductVersion" "${SLDSHOW_PRODUCT_VERSION}"
VIAddVersionKey "FileVersion" "${SLDSHOW_PRODUCT_VERSION}"
VIAddVersionKey "LegalCopyright" ""
VIAddVersionKey "FileDescription" "${SLDSHOW_NAME} Installer (64-bit)"

!define MUI_ABORTWARNING

!define MUI_ICON "..\assets\icon\icon.ico"
!define MUI_UNICON "..\assets\icon\icon.ico"

!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY

!define MUI_STARTMENUPAGE_REGISTRY_ROOT "HKCU"
!define MUI_STARTMENUPAGE_REGISTRY_KEY "Software\sldshow"
!define MUI_STARTMENUPAGE_REGISTRY_VALUENAME "Start Menu Folder"

!define MUI_FINISHPAGE_RUN "$INSTDIR\sldshow.exe"
!define MUI_FINISHPAGE_RUN_NOTCHECKED

Var StartMenuFolder
!insertmacro MUI_PAGE_STARTMENU Application $StartMenuFolder
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Japanese"

Section
    SetOutPath "$INSTDIR"
    File "..\target\release\sldshow.exe"
    File "..\README.md"
    File "..\LICENSE"
    File "..\example.sldshow"
    WriteUninstaller "$INSTDIR\uninstall.exe"

    # Computing EstimatedSize
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0

    WriteRegStr HKCU "${SLDSHOW_REG_KEY}" "" "$INSTDIR"
    WriteRegStr HKCU "${SLDSHOW_UNINST_KEY}" "DisplayIcon" "$INSTDIR\sldshow.exe"
    WriteRegStr HKCU "${SLDSHOW_UNINST_KEY}" "DisplayName" "${SLDSHOW_NAME}"
    WriteRegStr HKCU "${SLDSHOW_UNINST_KEY}" "DisplayVersion" "${SLDSHOW_SEMANTIC_VERSION}"
    WriteRegStr HKCU "${SLDSHOW_UNINST_KEY}" "Publisher" "ugai"
    WriteRegStr HKCU "${SLDSHOW_UNINST_KEY}" "Readme" "$INSTDIR\README.md"
    WriteRegStr HKCU "${SLDSHOW_UNINST_KEY}" "URLInfoAbout" "${SLDSHOW_HOMEPAGE}"
    WriteRegStr HKCU "${SLDSHOW_UNINST_KEY}" "UninstallString" "$INSTDIR\${SLDSHOW_UNINST_EXE}"
    WriteRegStr HKCU "${SLDSHOW_UNINST_KEY}" "QuietUninstallString" "$\"$INSTDIR\${SLDSHOW_UNINST_EXE}$\" /S"
    WriteRegDWORD HKCU "${SLDSHOW_UNINST_KEY}" "EstimatedSize" "$0"
    WriteRegDWORD HKCU "${SLDSHOW_UNINST_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${SLDSHOW_UNINST_KEY}" "NoRepair" 1

    !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
        CreateShortcut  "$SMPROGRAMS\sldshow.lnk" "$INSTDIR\sldshow.exe"
    !insertmacro MUI_STARTMENU_WRITE_END

    # File Association
    WriteRegStr HKCU "SOFTWARE\Classes\.sldshow" "" "sldshowfile"
    WriteRegStr HKCU "SOFTWARE\Classes\sldshowfile" "" "sldshow"
    WriteRegStr HKCU "SOFTWARE\Classes\sldshowfile\shell" "" "open"
    WriteRegStr HKCU "SOFTWARE\Classes\sldshowfile\shell\open\command" "" '"$INSTDIR\sldshow.exe" "%1"'
SectionEnd

Section "Uninstall"
    Delete   "$INSTDIR\uninstall.exe"
    RMDir /r "$INSTDIR"

    Delete   "$SMPROGRAMS\sldshow.lnk"

    DeleteRegKey HKCU "${SLDSHOW_REG_KEY}"
    DeleteRegKey HKCU "${SLDSHOW_UNINST_KEY}"

    # File Association
    DeleteRegKey HKCU "SOFTWARE\Classes\.sldshow"
    DeleteRegKey HKCU "SOFTWARE\Classes\sldshowfile"
SectionEnd
