$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$output=Join-Path $repo 'dist/Nisaba-CV-v1.0-source.zip'
$roots=@('.github','assets','crates','docs','packaging','scripts','third-party')
$names=@('.gitattributes','.gitignore','Cargo.lock','Cargo.toml','CHANGELOG.md','CONTRIBUTING.md','LICENSE','README.md','runtime-lock.json','rust-toolchain.toml')
$files=@($names | ForEach-Object {Get-Item -LiteralPath (Join-Path $repo $_)})
foreach($root in $roots){$files+=Get-ChildItem -LiteralPath (Join-Path $repo $root) -File -Force -Recurse}
foreach($file in $files){
 if($file.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Source archive cannot contain links'}
 if($file.Name -match '\.(exe|dll|sqlite3|rslbackup)$' -or $file.FullName -match '[\\/]nisaba-data[\\/]'){throw 'Source archive cannot contain runtime binaries or personal data'}
}
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $output) | Out-Null
$stream=[IO.File]::Open($output,[IO.FileMode]::Create,[IO.FileAccess]::Write,[IO.FileShare]::None)
$zip=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Create)
try{
 foreach($file in ($files | Sort-Object FullName)){
  $entry='nisaba-cv/'+[IO.Path]::GetRelativePath($repo,$file.FullName).Replace('\','/')
  [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,$file.FullName,$entry,[IO.Compression.CompressionLevel]::Optimal) | Out-Null
 }
}finally{$zip.Dispose();$stream.Dispose()}
Write-Output $output
