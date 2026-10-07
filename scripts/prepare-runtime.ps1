param([switch]$Offline)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$runtime=Join-Path $repo 'runtime'
$lock=Get-Content -LiteralPath (Join-Path $repo 'runtime-lock.json') -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Force -Path (Join-Path $runtime 'fonts'),(Join-Path $runtime 'tools') | Out-Null
Get-ChildItem -LiteralPath (Join-Path $repo 'assets/fonts') -File | ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $runtime 'fonts')}
$typst=Join-Path $runtime 'tools/typst.exe'
if(-not(Test-Path -LiteralPath $typst) -or (Get-FileHash -LiteralPath $typst).Hash -ne $lock.exeSha256){
 if($Offline){throw 'Pinned Typst is absent. Run prepare-runtime.ps1 with network access first.'}
 $cache=Join-Path $repo '.cache'
 New-Item -ItemType Directory -Force -Path $cache | Out-Null
 $archive=Join-Path $cache ('typst-'+$lock.typstVersion+'.zip')
 if(-not(Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive).Hash -ne $lock.archiveSha256){Invoke-WebRequest -Uri $lock.archiveUrl -OutFile $archive}
 if((Get-FileHash -LiteralPath $archive).Hash -ne $lock.archiveSha256){throw 'Typst archive SHA-256 mismatch'}
 $extracted=Join-Path $cache ('typst-extracted-'+[Guid]::NewGuid())
 Expand-Archive -LiteralPath $archive -DestinationPath $extracted
 $source=Join-Path $extracted 'typst-x86_64-pc-windows-msvc'
 Copy-Item -LiteralPath (Join-Path $source 'typst.exe') -Destination $typst
 Copy-Item -LiteralPath (Join-Path $source 'LICENSE') -Destination (Join-Path $repo 'third-party/Typst-LICENSE.txt')
 Copy-Item -LiteralPath (Join-Path $source 'NOTICE') -Destination (Join-Path $repo 'third-party/Typst-NOTICE.txt')
}
if((Get-FileHash -LiteralPath $typst).Hash -ne $lock.exeSha256){throw 'Typst executable SHA-256 mismatch'}
$crt=Join-Path $runtime 'tools/vcruntime140.dll'
if(-not(Test-Path -LiteralPath $crt)){
 $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
 if(-not(Test-Path -LiteralPath $vswhere)){throw 'Visual Studio redistributable directory is required for app-local CRT.'}
 $installation=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
 if(-not $installation){throw 'Visual Studio C++ desktop component is required.'}
 $redistRoot=Join-Path $installation 'VC/Redist/MSVC'
 $candidate=Get-ChildItem -LiteralPath $redistRoot -Directory | Sort-Object {[Version]$_.Name} -Descending | ForEach-Object {Get-ChildItem -LiteralPath (Join-Path $_.FullName 'x64') -Filter 'vcruntime140.dll' -File -Recurse} | Select-Object -First 1
 if(-not $candidate){throw 'Redistributable vcruntime140.dll was not found.'}
 Copy-Item -LiteralPath $candidate.FullName -Destination $crt
}
Write-Output 'Runtime prepared; no system component was installed.'
