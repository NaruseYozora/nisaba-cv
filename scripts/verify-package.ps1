$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$package=Join-Path $repo 'dist/Nisaba-CV-v1.1-windows-x64.exe'
$manifest=(Get-Content -LiteralPath (Join-Path $repo 'dist/manifest.json') -Raw | ConvertFrom-Json).files
$checkRoot=Join-Path $repo ('.test-output/package-check-'+[Guid]::NewGuid())
New-Item -ItemType Directory -Path $checkRoot | Out-Null
$localPackage=Join-Path $checkRoot 'Nisaba-CV-v1.1-windows-x64.exe'
Copy-Item -LiteralPath $package -Destination $localPackage
$env:HTTP_PROXY='http://127.0.0.1:9'
$env:HTTPS_PROXY=$env:HTTP_PROXY
$env:ALL_PROXY=$env:HTTP_PROXY
$env:NO_PROXY=''
$env:PATH="$env:SystemRoot\System32;$env:SystemRoot"
$env:WEBVIEW2_BROWSER_EXECUTABLE_FOLDER=Join-Path $checkRoot 'missing-webview'
function Run-Checked([string]$exe,[string[]]$arguments){
 $p=Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru
 if(-not $p.WaitForExit(60000)){throw 'Package check timed out'}
 return $p.ExitCode
}
function Check-Files([string]$target){
 foreach($entry in $manifest){
  $file=Join-Path $target $entry.path
  if(-not(Test-Path -LiteralPath $file) -or (Get-FileHash -LiteralPath $file).Hash -ne $entry.sha256){throw "Extracted file differs: $($entry.path)"}
 }
 if(@(Get-ChildItem -LiteralPath $target -File -Recurse).Count -ne $manifest.Count){throw 'Unexpected extracted files'}
}
# No /D override: verify the public, version-free default directory itself.
if((Run-Checked $localPackage @('/S')) -ne 0){throw 'Default extraction failed'}
$target=Join-Path $checkRoot 'Nisaba CV'
Check-Files $target
$sentinel=Join-Path $target 'keep-user-file.txt'
[IO.File]::WriteAllText($sentinel,'anonymous retention sentinel')
if((Run-Checked $localPackage @('/S')) -eq 0){throw 'Populated directory accepted'}
if([IO.File]::ReadAllText($sentinel) -ne 'anonymous retention sentinel'){throw 'User file changed'}
foreach($entry in $manifest){if((Get-FileHash -LiteralPath (Join-Path $target $entry.path)).Hash -ne $entry.sha256){throw 'Rejected extraction changed application'}}
$empty=Join-Path $checkRoot '已有空文件夹 带空格'
New-Item -ItemType Directory -Path $empty | Out-Null
if((Run-Checked $localPackage @('/S',('/D='+$empty))) -ne 0){throw 'Existing empty folder rejected'}
Check-Files $empty
$hidden=Join-Path $checkRoot 'hidden-file-test'
New-Item -ItemType Directory -Path $hidden | Out-Null
$hiddenFile=Join-Path $hidden 'keep.txt'
[IO.File]::WriteAllText($hiddenFile,'keep hidden file')
[IO.File]::SetAttributes($hiddenFile,[IO.FileAttributes]::Hidden)
if((Run-Checked $localPackage @('/S',('/D='+$hidden))) -eq 0){throw 'Hidden file ignored'}
if(@(Get-ChildItem -LiteralPath $hidden -Force).Count -ne 1 -or [IO.File]::ReadAllText($hiddenFile) -ne 'keep hidden file'){throw 'Hidden file changed'}
if((Run-Checked $localPackage @('/S',('/D='+$sentinel))) -eq 0){throw 'File path accepted'}
$data=Join-Path $checkRoot 'anonymous-data'
$exe=Join-Path $target 'Nisaba-CV.exe'
if((Run-Checked $exe @('--self-test','--data',('"'+$data+'"'))) -ne 0){throw 'Extracted application self-test failed'}
$test=Get-Content -LiteralPath (Join-Path $data 'self-test.json') -Raw | ConvertFrom-Json
if(-not $test.passed -or $test.managedPreviewDirectories -ne 0){throw 'Application or cache lifecycle check failed'}
$before=(Get-FileHash -LiteralPath (Join-Path $data 'library.sqlite3')).Hash
if((Run-Checked $exe @('--self-test','--data',('"'+$data+'"'))) -eq 0){throw 'Self-test accepted existing personal data'}
if((Get-FileHash -LiteralPath (Join-Path $data 'library.sqlite3')).Hash -ne $before){throw 'Rejected self-test changed data'}
$version=(Get-Item -LiteralPath $package).VersionInfo
if($version.ProductVersion -ne '1.1' -or $version.FileVersion -ne '1.1'){throw 'Public package version differs'}
[ordered]@{passed=$true;version=$version.ProductVersion;defaultFolder='Nisaba CV';files=$manifest.Count;packageBytes=(Get-Item -LiteralPath $package).Length;packageSha256=(Get-FileHash -LiteralPath $package).Hash;exeSha256=(Get-FileHash -LiteralPath $exe).Hash;newAndEmptyDirectoriesPassed=$true;existingAndHiddenFilesProtected=$true;selfTest=$test;physicalNetworkDisconnect=$false} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $repo 'dist/verification.json') -Encoding utf8
Get-Content -LiteralPath (Join-Path $repo 'dist/verification.json')
