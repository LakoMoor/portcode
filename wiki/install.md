# Install

## The installer (macOS, Linux, WSL)

On Windows, scroll to [Windows (PowerShell)](#windows-powershell).

```sh
curl -fsSL https://raw.githubusercontent.com/LakoMoor/portcode/main/install.sh | sh
portcode --version
```

The installer downloads the archive for your system from the latest release and **verifies its sha256** before it installs anything. It puts the binary in `~/.local/bin` (set `PORTCODE_INSTALL_DIR` to change that) and tells you if the folder is not on your `PATH`.

| System | Archive |
|---|---|
| macOS, Apple silicon | `portcode-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `portcode-x86_64-apple-darwin.tar.gz` |
| Linux, x86_64 | `portcode-x86_64-unknown-linux-gnu.tar.gz` |
| Linux, aarch64 | `portcode-aarch64-unknown-linux-gnu.tar.gz` |

Linux archives need glibc 2.34 or newer (Ubuntu 22.04, Debian 12). On a minimal Linux install `ca-certificates` too, or the download cannot make a TLS connection.

## Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/LakoMoor/portcode/main/install.ps1 | iex
portcode --version
```

No Rust and no Visual C++ runtime are needed: `portcode.exe` is a single static file. The script downloads `portcode-x86_64-pc-windows-msvc.zip` (or `portcode-aarch64-pc-windows-msvc.zip` on ARM), **verifies its sha256**, copies `portcode.exe` to `%LOCALAPPDATA%\Programs\portcode\bin` and adds that folder to your user `PATH`. Open a new terminal afterwards.

| Variable | Meaning |
|---|---|
| `PORTCODE_VERSION` | install a specific release, for example `v0.1.6` |
| `PORTCODE_INSTALL_DIR` | another folder for `portcode.exe` |
| `PORTCODE_UNINSTALL=1` | remove `portcode.exe` and the `PATH` entry |

Set them before the command, for example `$env:PORTCODE_VERSION = "v0.1.6"`.

The `bash` tool runs commands with `sh` from [Git for Windows](https://git-scm.com/download/win) (`winget install Git.Git`) when it is there, and with `cmd.exe` otherwise. The deep-mode engine installers (`portcode engines install core`) need Git for Windows. WSL users can use the Linux installer instead.

## From source

You need a recent stable Rust toolchain.

```sh
cargo install --git https://github.com/LakoMoor/portcode portcode-cli
```

## Updating

```sh
portcode update
```

`portcode update` (or `/update` in the app) compares your version with the latest release and prints the install command. Once a day the app checks quietly and only speaks up if a newer version exists. Set `PORTCODE_NO_UPDATE_CHECK=1` to turn the check off. Nothing is ever installed behind your back.

## Check the setup

```sh
portcode doctor
```

`doctor` tests each provider, your Rust toolchain and the optional engines, and says what to fix. Continue with [First run](first-run.html).

## Shell completions

```sh
portcode completions zsh > ~/.zfunc/_portcode
```

Supported shells: bash, zsh, fish, powershell, elvish.
