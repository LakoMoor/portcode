# Build the Windows release archive: packaging\package.ps1 [target]  ->  dist\portcode-<target>.zip (+ .sha256)
param([string]$Target = 'x86_64-pc-windows-msvc')
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
cargo build --release --locked -p portcode-cli --target $Target
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
New-Item -ItemType Directory -Force -Path dist | Out-Null
$stage = Join-Path ([IO.Path]::GetTempPath()) ("pc-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item "target/$Target/release/portcode.exe", 'LICENSE', 'NOTICE', 'README.md' -Destination $stage
$zip = "dist/portcode-$Target.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
$h = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
[IO.File]::WriteAllText("$zip.sha256", "$h  portcode-$Target.zip`n")
Remove-Item -Recurse -Force $stage
Get-ChildItem dist
