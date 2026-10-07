Unicode true
!include "FileFunc.nsh"
!ifndef BUNDLE_DIR
 !define BUNDLE_DIR "..\dist\Nisaba CV"
!endif
!ifndef OUTPUT_FILE
 !define OUTPUT_FILE "..\dist\Nisaba-CV-v1.1-windows-x64.exe"
!endif
Name "Nisaba CV 1.1"
OutFile "${OUTPUT_FILE}"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 128
InstallDir "$EXEDIR\Nisaba CV"
LoadLanguageFile "${NSISDIR}\Contrib\Language files\SimpChinese.nlf"
VIProductVersion "1.1.0.0"
VIAddVersionKey /LANG=2052 "ProductName" "Nisaba CV"
VIAddVersionKey /LANG=2052 "ProductVersion" "1.1"
VIAddVersionKey /LANG=2052 "FileVersion" "1.1"
VIAddVersionKey /LANG=2052 "FileDescription" "Nisaba CV 便携自解压包"
VIAddVersionKey /LANG=2052 "LegalCopyright" "Copyright 2026 Nisaba CV contributors"
DirText "选择新的目录或已有空文件夹，例如 D:\Nisaba CV。此包只解压程序；包含文件的目录受到保护。"
Page directory "" "" CheckDestination
Page instfiles

Function CheckDestination
 Push $0
 System::Call 'kernel32::GetFileAttributesW(w "$INSTDIR") i.r0'
 IntCmp $0 -1 allowed
 IntOp $0 $0 & 0x10
 IntCmp $0 0 blocked
 ${DirState} "$INSTDIR" $0
 IntCmp $0 0 allowed blocked blocked
 blocked:
  Pop $0
  MessageBox MB_OK|MB_ICONEXCLAMATION "目标包含文件、无法读取，或该路径是文件。请选择新目录或空文件夹，例如 D:\Nisaba CV。" /SD IDOK
  SetErrorLevel 2
  Abort
 allowed:
  Pop $0
  SetErrorLevel 0
FunctionEnd
Function .onInit
 IfSilent silent done
 silent:
  Call CheckDestination
 done:
FunctionEnd
Section
 Call CheckDestination
 SetOutPath "$INSTDIR"
 File "${BUNDLE_DIR}\Nisaba-CV.exe"
 File "${BUNDLE_DIR}\README.md"
 File "${BUNDLE_DIR}\LICENSE"
 SetOutPath "$INSTDIR\tools"
 File /r "${BUNDLE_DIR}\tools\*"
 SetOutPath "$INSTDIR\fonts"
 File /r "${BUNDLE_DIR}\fonts\*"
 SetOutPath "$INSTDIR\licenses"
 File /r "${BUNDLE_DIR}\licenses\*"
SectionEnd
