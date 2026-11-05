# PortCode wiki

PortCode is a terminal agent that studies a program, rewrites it in Rust, and checks the result against the original. It is built for models you run yourself (Ollama, LM Studio, llama.cpp, vLLM), and it also works with a hosted API or with your Claude Code or Codex subscription.

What sets it apart is the proof. PortCode pins the exact build of the original (sha256), writes down every fact as an *observation*, an *inference* or an *unknown*, ports one module at a time, and only calls a module done after a `parity` run passes against the original's own output.

## Where to start

| If you want to… | Read |
|---|---|
| install it | [Install](install.html) |
| pick a model and enter a key | [First run](first-run.html) |
| see what the screen shows and how to drive it | [A tour of the app](tour.html) |
| port a program | [The port workflow](port-workflow.html) |
| look inside a binary, a Unity game or a .NET app | [Clean-room and deep mode](modes.html), [Engines](engines.html) |
| use your Claude or ChatGPT subscription | [Claude Code and Codex](subscriptions.html) |
| run local models well | [Providers and models](providers.html) |
| look something up | [Slash commands](commands.html), [Keyboard shortcuts](keys.html), [Configuration](config.html), [Command line](cli.html) |

## The pieces

- **Agents** decide what the model does: `build` for general work, `port` for the full port workflow, `reverse` for studying an original, `plan` for read-only planning, and `team` for several workers at once. See [Agents](agents.html).
- **Tools** are everything the agent can do: read and edit files, run commands, record evidence, run parity checks, take screenshots, build the site. The app, the agent and `portcode tool` share one list. See [Tools](tools.html).
- **Modes** decide how closely the original may be examined. The default, *clean-room*, only observes it from outside. *Deep* mode may disassemble, decompile and emulate it.
- **Permissions** keep you in charge: file edits and shell commands ask first unless you allow them. See [Permissions](permissions.html).
- **The ledger** is the project's memory of facts about the original, kept in `.portcode/evidence/`.

> [!NOTE]
> PortCode is an alpha. Study only software you own or have permission to examine.

## How these pages are organised

*Start* gets you running. *Use* covers daily work in the app. *Port and reverse* explains the workflow that makes PortCode different. *Models* covers providers. *Reference* lists every command, key and setting. *Project* is for people who want to read or change the code.
