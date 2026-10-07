$ErrorActionPreference='Stop'
if(-not(Get-Command cargo -ErrorAction SilentlyContinue)){throw 'Install Rust (rustup) before building.'}
if(-not(Get-Command cl.exe -ErrorAction SilentlyContinue)){
 $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
 if(-not(Test-Path -LiteralPath $vswhere)){throw 'Visual Studio C++ desktop tools are required.'}
 $installation=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
 if(-not $installation){throw 'MSVC x64/x86 and Windows SDK are required.'}
 Import-Module (Join-Path $installation 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
 Enter-VsDevShell -VsInstallPath $installation -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
}
