# Troubleshooting

Start with `portcode doctor`: it tests providers, the Rust toolchain and the engines, and says what to fix.

## Install and update

**`GLIBC_2.39 not found` on Linux.** This only affected the first upload of v0.1.0. Current archives need glibc 2.34 or newer (Ubuntu 22.04, Debian 12). Run the installer again, or build with `cargo install --git https://github.com/LakoMoor/portcode portcode-cli`.

**`could not set up TLS` or "builder error" on a minimal Linux.** Install `ca-certificates`.

**`portcode: command not found` after installing.** The installer puts the binary in `~/.local/bin`. Add that folder to your `PATH`, or set `PORTCODE_INSTALL_DIR` and install again.

## Models and keys

**No API key.** Type `/key`, or choose a local provider with `/provider`. Local servers need no key.

**Connection refused.** The local server is not running, or the port in `portcode.json` differs from the server's.

**Model not found.** `/models` shows the exact names the server offers.

**The connection drops.** A lost or stalled connection to the model server, or a 429/5xx answer, is retried by itself: pauses of 2, 4, 8, 15 and then 30 seconds, up to 12 attempts, shown in the chat. `esc` stops it. The phone page of `/remote` also reconnects on its own and right away when the phone wakes or the network returns.

**Small context window.** PortCode asks the server for the real size (vLLM, OpenRouter, llama.cpp, LM Studio, Ollama report it) and says so when it differs from its built-in guess. If your server does not report it, set `context_window` for the model under `"models"` in `portcode.json`. PortCode then shortens tool output and compacts old turns to fit. See [Providers and models](providers.html).

**The model writes tool calls as plain text, or breaks JSON.** PortCode repairs the common cases. If a model still struggles, run `portcode eval` against it and try `/style compact`. For Qwen3.8 models see the profile notes in [Providers and models](providers.html).

## In the app

**Nothing happens when I press `esc`.** Cancelling needs two presses within two seconds. The footer shows *press esc again to cancel* after the first. `ctrl+c` cancels at once.

**The agent stopped and shows "Needs your decision".** It cannot continue without you. Answer the questions in the prompt and it carries on.

**It keeps asking for permission.** Answer `a` to always allow that exact request for the session, add `allow` rules in `portcode.json` (see [Permissions](permissions.html)), or use `/yolo` in a project you trust.

**No suggestion appears in the prompt.** It only shows in a fresh session with an empty prompt, and `"suggest": false` turns it off. If the provider has no key you get a local guess, not a model's.

**`/team` says nothing happens.** Switch to the `team` agent with `tab` after turning team mode on.

**The `venv` tool says virtual environments are off.** `"venv": false` is set, or you ran `/venv off`. Use `/venv on`.

**`python3 -m venv` fails.** Install Python 3 and, on Debian and Ubuntu, `python3-venv`.

## Deep mode

**A deep tool says it needs deep mode.** Start with `--deep` or type `/deep`.

**An engine is missing.** `portcode engines list` shows what is installed. `portcode engines install core` installs Capstone, Unicorn, LIEF and Kuna. Engines that are not Python packages (rizin and others) are installed with your package manager; the list shows the command.

## Subscriptions

**Claude Code or Codex is not found.** Install the official CLI and sign in once, then check `portcode doctor`. See [Claude Code and Codex](subscriptions.html).

## Still stuck

Open an issue at [github.com/LakoMoor/portcode/issues](https://github.com/LakoMoor/portcode/issues) with the output of `portcode doctor` and the version from `portcode --version`. Remove keys and personal paths first.

## Windows

- **`portcode` is not recognised right after install.** Open a new terminal: the installer changes your user `PATH`, which running terminals do not see.
- **The installer says it could not write `portcode.exe`.** PortCode is running. Close it and run the installer again.
- **`bash` tool: "no POSIX shell".** Install Git for Windows (`winget install Git.Git`) so the agent can use `sh`; without it only simple `cmd.exe` commands work.
- **SmartScreen warns about `portcode.exe`.** The binary is not code-signed yet. Check the sha256 against `SHA256SUMS` on the release page; the installer does this for you.
