param([switch]$Offline)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'enable-build-tools.ps1')
& (Join-Path $PSScriptRoot 'prepare-runtime.ps1') -Offline:$Offline
$flags=$env:RUSTFLAGS
try{
 $env:RUSTFLAGS=($flags+' -C target-feature=+crt-static').Trim()
 $arguments=@('build','--manifest-path',(Join-Path $repo 'Cargo.toml'),'--release','--locked','--bin','nisaba-cv')
 if($Offline){$arguments+='--offline'}
 & cargo @arguments
 if($LASTEXITCODE -ne 0){throw 'Release build failed'}
 $target=if($env:CARGO_TARGET_DIR){[IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)}else{Join-Path $repo 'target'}
 $bundle=Join-Path $repo 'dist/Nisaba CV'
 if(Test-Path -LiteralPath (Join-Path $bundle 'nisaba-data')){throw 'Refusing to rebuild a directory containing user data'}
 New-Item -ItemType Directory -Force -Path $bundle,(Join-Path $bundle 'fonts'),(Join-Path $bundle 'tools'),(Join-Path $bundle 'licenses/rust') | Out-Null
 Copy-Item -LiteralPath (Join-Path $target 'release/nisaba-cv.exe') -Destination (Join-Path $bundle 'Nisaba-CV.exe')
 Get-ChildItem -LiteralPath (Join-Path $repo 'runtime/tools') -File | ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $bundle 'tools')}
 Get-ChildItem -LiteralPath (Join-Path $repo 'assets/fonts') -File | ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $bundle 'fonts')}
 Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination $bundle
 Copy-Item -LiteralPath (Join-Path $repo 'packaging/README.md') -Destination (Join-Path $bundle 'README.md')
 Get-ChildItem -LiteralPath (Join-Path $repo 'third-party') -File | Where-Object {$_.Name -like 'Typst-*' -or $_.Name -like 'MSVC-*'} | ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $bundle 'licenses')}
 $metadataArgs=@('metadata','--manifest-path',(Join-Path $repo 'Cargo.toml'),'--locked','--format-version','1','--filter-platform','x86_64-pc-windows-msvc')
 if($Offline){$metadataArgs+='--offline'}
 $metadata=(& cargo @metadataArgs) | ConvertFrom-Json
 if($LASTEXITCODE -ne 0){throw 'Dependency metadata failed'}
 $resolved=@{};foreach($node in $metadata.resolve.nodes){$resolved[$node.id]=$true}
 $dependencies=@()
 foreach($package in ($metadata.packages | Where-Object {$resolved.ContainsKey($_.id) -and $_.source} | Sort-Object name,version)){
  $name=$package.name+'-'+$package.version
  $destination=Join-Path $bundle ('licenses/rust/'+$name)
  New-Item -ItemType Directory -Force -Path $destination | Out-Null
  Get-ChildItem -LiteralPath (Split-Path -Parent $package.manifest_path) -File | Where-Object {$_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE|OFL|UFL|COPYRIGHT)'} | ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination $destination}
  $supplement=Join-Path $repo ('third-party/rust/'+$name)
  if(Test-Path -LiteralPath $supplement){Get-ChildItem -LiteralPath $supplement -File | ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination $destination}}
  $notices=@(Get-ChildItem -LiteralPath $destination -File)
  # Some crates include a short LICENSE pointer beside the full license texts.
  if(@($notices | Where-Object Length -GE 20).Count -eq 0){throw "Missing license text: $name"}
  $dependencies += [ordered]@{name=$package.name;version=$package.version;license=$package.license;repository=$package.repository;licenseFiles=@($notices.Name)}
 }
 $dependencies | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $bundle 'licenses/Rust-dependencies.json') -Encoding utf8
 $files=@(Get-ChildItem -LiteralPath $bundle -File -Recurse | ForEach-Object {[ordered]@{path=[IO.Path]::GetRelativePath($bundle,$_.FullName);bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash}})
 [ordered]@{version='1.1.0';folderName='Nisaba CV';files=$files} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $repo 'dist/manifest.json') -Encoding utf8
 Write-Output $bundle
}finally{$env:RUSTFLAGS=$flags}
