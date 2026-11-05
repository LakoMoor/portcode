# Fails when portcode.exe imports the Visual C++ runtime (it would not start on a clean Windows install).
param([Parameter(Mandatory)][string]$Exe)
$ErrorActionPreference = 'Stop'
$out = & $Exe --version
if ($LASTEXITCODE -ne 0) { throw 'portcode.exe did not start' }
Write-Host $out
$txt = [Text.Encoding]::Latin1.GetString([IO.File]::ReadAllBytes($Exe))
$bad = [regex]::Matches($txt, '(?i)(vcruntime\d*|msvcp\d*|ucrtbase|api-ms-win-crt-[a-z-]+)\.dll') | ForEach-Object { $_.Value } | Sort-Object -Unique
if ($bad) { throw "the binary imports the Visual C++ runtime: $($bad -join ', ')" }
Write-Host 'static runtime: no VC++ DLL imports'
