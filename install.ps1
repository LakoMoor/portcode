# PortCode installer for Windows (PowerShell 5.1 or newer). No Rust needed: it downloads a prebuilt portcode.exe
# and verifies its sha256 before installing anything.
#
#   irm https://raw.githubusercontent.com/LakoMoor/portcode/main/install.ps1 | iex
#
# Options (environment variables, set them before the command):
#   $env:PORTCODE_VERSION     = "v0.1.6"   install a specific release (default: latest)
#   $env:PORTCODE_INSTALL_DIR = "C:\tools" where to put portcode.exe (default: %LOCALAPPDATA%\Programs\portcode\bin)
#   $env:PORTCODE_DOWNLOAD_URL = "..."     another base URL, or a folder, for the assets (mirrors, testing)
#   $env:PORTCODE_UNINSTALL   = "1"        remove portcode.exe and its PATH entry (config and project .portcode folders stay)
# The user PATH lives in the registry as REG_EXPAND_SZ and usually holds entries like %USERPROFILE%\bin.
# Reading it through [Environment]::GetEnvironmentVariable expands them, and writing it back through
# SetEnvironmentVariable stores the expanded text as plain REG_SZ: other tools' entries get frozen or mangled.
# So read the raw value, change only our own entry, and write it back with the same type.
function Open-EnvKey([bool]$write) {
    [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $write)
}
function Get-UserPathEntries {
    $k = Open-EnvKey $false
    try { $raw = $k.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $k.Close() }
    if ($raw -isnot [string]) { return @() }
    return @($raw.Split(';') | Where-Object { $_.Trim() -ne '' })
}
function Set-UserPathEntries($entries) {
    $entries = @($entries)
    $k = Open-EnvKey $true
    try {
        if ($entries.Count -eq 0) { $k.DeleteValue('Path', $false) }
        else { $k.SetValue('Path', ($entries -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString) }
    } finally { $k.Close() }
    # tell running programs (Explorer, new terminals) that the environment changed
    try {
        Add-Type -Namespace PortCode -Name Native -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError=true, CharSet=System.Runtime.InteropServices.CharSet.Auto)] public static extern System.IntPtr SendMessageTimeout(System.IntPtr h, uint m, System.UIntPtr w, string l, uint f, uint t, out System.UIntPtr r);' -ErrorAction Stop
        $r = [UIntPtr]::Zero
        [void][PortCode.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 2000, [ref]$r)
    } catch {}
}
function Expand-Entry([string]$e) { [Environment]::ExpandEnvironmentVariables($e) }

function Install-PortCode {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'   # the progress bar makes downloads many times slower in Windows PowerShell
    try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch {}

    $repo = 'LakoMoor/portcode'
    $dir = if ($env:PORTCODE_INSTALL_DIR) { $env:PORTCODE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\portcode\bin' }
    $parts = @(Get-UserPathEntries)

    if ($env:PORTCODE_UNINSTALL -eq '1') {
        Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $dir 'portcode.exe')
        $keep = @($parts | Where-Object { (Expand-Entry $_).TrimEnd('\') -ne $dir.TrimEnd('\') })
        if ($keep.Count -ne $parts.Count) { Set-UserPathEntries $keep }
        Write-Host "removed $dir\portcode.exe (config in %APPDATA%\portcode and project .portcode folders were left alone)"
        return
    }

    $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    switch ($arch) {
        'AMD64' { $target = 'x86_64-pc-windows-msvc' }
        'ARM64' { $target = 'aarch64-pc-windows-msvc' }
        default { throw "unsupported CPU '$arch'. PortCode for Windows needs 64-bit x86 or ARM." }
    }
    $asset = "portcode-$target.zip"
    if ($env:PORTCODE_DOWNLOAD_URL) { $base = $env:PORTCODE_DOWNLOAD_URL }
    elseif ($env:PORTCODE_VERSION) { $base = "https://github.com/$repo/releases/download/$($env:PORTCODE_VERSION)" }
    else { $base = "https://github.com/$repo/releases/latest/download" }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("portcode-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Write-Host "downloading $asset..."
        foreach ($f in @($asset, "$asset.sha256")) {
            if (Test-Path -LiteralPath $base -PathType Container) { Copy-Item -LiteralPath (Join-Path $base $f) -Destination (Join-Path $tmp $f) }
            else { Invoke-WebRequest -UseBasicParsing -Uri "$base/$f" -OutFile (Join-Path $tmp $f) }
        }
        $want = ((Get-Content -LiteralPath (Join-Path $tmp "$asset.sha256") -Raw).Trim() -split '\s+')[0].ToLower()
        $got = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $tmp $asset)).Hash.ToLower()
        if ($want -ne $got) { throw "checksum mismatch for $asset (expected $want, got $got). Refusing to install." }
        Write-Host 'checksum verified'
        Expand-Archive -LiteralPath (Join-Path $tmp $asset) -DestinationPath $tmp -Force
        $exe = Join-Path $tmp 'portcode.exe'
        if (-not (Test-Path -LiteralPath $exe)) { throw 'the archive did not contain portcode.exe' }
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        try { Copy-Item -LiteralPath $exe -Destination (Join-Path $dir 'portcode.exe') -Force }
        catch { throw "could not write $dir\portcode.exe. Is PortCode running? Close it and run the installer again." }
    }
    finally { Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp }

    $onPath = @($parts | Where-Object { (Expand-Entry $_).TrimEnd('\') -eq $dir.TrimEnd('\') })
    if ($onPath.Count -eq 0) {
        Set-UserPathEntries (@($parts) + @($dir))
        $env:Path = "$env:Path;$dir"
        Write-Host "added $dir to your PATH (new terminals pick it up)"
    }
    $ver = try { & (Join-Path $dir 'portcode.exe') --version } catch { 'unknown version' }
    Write-Host "installed: $dir\portcode.exe ($ver)"
    Write-Host 'next:  portcode doctor   then   portcode   (in the folder you want to work in)'
}
try { Install-PortCode } catch { Write-Host "error: $($_.Exception.Message)" -ForegroundColor Red }
