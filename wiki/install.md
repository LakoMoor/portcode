# Install

## The installer (macOS, Linux, WSL)

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

Windows is supported through WSL.

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
