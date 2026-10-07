param([string]$Makensis='makensis.exe')
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$bundle=Join-Path $repo 'dist/Nisaba CV'
if(-not(Test-Path -LiteralPath (Join-Path $bundle 'README.md'))){throw 'Run scripts/build.ps1 first'}
if(Test-Path -LiteralPath (Join-Path $bundle 'nisaba-data')){throw 'Refusing to package user data'}
$exe=Join-Path $repo 'dist/Nisaba-CV-v1.1-windows-x64.exe'
& $Makensis /INPUTCHARSET UTF8 /V2 ('/DBUNDLE_DIR='+$bundle) ('/DOUTPUT_FILE='+$exe) (Join-Path $repo 'packaging/portable.nsi')
if($LASTEXITCODE -ne 0){throw 'NSIS packaging failed'}
$artifacts=@(Get-Item -LiteralPath $exe)
$artifacts | ForEach-Object {(Get-FileHash -LiteralPath $_.FullName).Hash.ToLower()+'  '+$_.Name} | Set-Content -LiteralPath (Join-Path $repo 'dist/SHA256SUMS.txt') -Encoding ascii
Write-Output $exe
