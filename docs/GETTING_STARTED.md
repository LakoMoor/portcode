# Getting started

Five minutes from install to your first port.

## 1. Install

```sh
curl -fsSL https://raw.githubusercontent.com/LakoMoor/portcode/main/install.sh | sh
portcode --version
```

The installer checks the sha256 of the download. Another way: `cargo install --git https://github.com/LakoMoor/portcode portcode-cli`. A Homebrew tap is planned.

## 2. Pick a model and give it a key

PortCode talks to any OpenAI-compatible server. Choose one:

- **A local model (no key).** Start Ollama, LM Studio, llama.cpp or vLLM, then run `portcode` and type `/provider ollama qwen3:14b`.
- **A hosted API.** Run `portcode`, type `/key` and paste your key. It is hidden while you type and saved in your config folder with mode 600. From a shell the same thing is `portcode auth set <provider>`, or `portcode auth set <provider> --from-env NAME` to read it from an environment variable.

Inside the app:

| Type | What it does |
|---|---|
| `/provider` | lists providers and shows which ones still need a key |
| `/provider <name> [model]` | switches provider (and model) |
| `/key [provider]` | asks for the API key, hidden |
| `/models` | lists the models the current provider serves |
| `/model <provider>/<model>` | picks a model |
| `/theme light` or `/theme dark` | switches the colours, remembered for next time (`PORTCODE_THEME` overrides) |
| `/setup` | prints these steps again |

The model you pick with `/model` or `/provider` is remembered for the next start (`-m` and `PORTCODE_MODEL` still win).

In the `/` menu use ↑ and ↓ to choose, `tab` or `enter` to complete.

## 3. Check it works

```sh
portcode doctor
```

It tests each provider, your Rust toolchain and the optional engines, and tells you what to fix. `portcode eval` probes the model itself (chat, tool calls, JSON, edits).

## 4. Start

Run `portcode` in the folder that holds the program you want to port, then type `/create`. It asks what to recreate, how closely PortCode may look inside it and what to build with, and starts the right agent. Or just type a task:

```
Port this CLI to Rust, following rustport
```

Put the original in `original/`. PortCode pins its sha256, records what it learns as observation, inference or unknown, and only calls a module done after `parity` passes.

## Updates

`portcode update` (or `/update` in the app) compares your version with the latest release and prints the install command. Once a day the app checks quietly and only says something if a newer version exists. Turn it off with `PORTCODE_NO_UPDATE_CHECK=1`. Nothing is installed for you.

## If something is wrong

- *`GLIBC_2.39 not found` on Linux* — this only affected the first upload of `v0.1.0`. The current archives need glibc 2.34 or newer (Ubuntu 22.04, Debian 12). If you still see it, run the installer again or build with `cargo install --git https://github.com/LakoMoor/portcode portcode-cli`.
- *`could not set up TLS` on a minimal Linux* — install `ca-certificates`.
- *No API key* — `/key`, or pick a local provider with `/provider`.
- *Connection refused* — the local server is not running, or the port in `portcode.json` differs.
- *Model not found* — `/models` shows the exact names.
- *Small context window* — set `context_window` for the model under `models` in `portcode.json`; see PROVIDERS.
