# Slash commands

Type `/` in the prompt to open the command menu. Use `↑` and `↓` to choose, `tab` or `enter` to complete; the filter matches as you type. An unknown command is an error, never a prompt sent to the model.

This table is generated from the app's own command list, so it matches the version you build the docs from.

{{slash}}

## Details

### Choosing a model

`/provider` lists providers and which ones lack a key. `/provider ollama qwen3:14b` switches provider and model. `/models` lists what the current provider serves; `/model ollama/qwen3:14b` picks one. `/key` asks for an API key without showing it. `/setup` repeats the first-run steps. See [First run](first-run.html).

### Tuning the prompt

`/style auto|compact|full|qwen38` picks the prompt flavour for the session and `/effort xhigh|medium|low|none` sets Qwen3.8 thinking depth (`none` turns thinking off). See [Providers and models](providers.html).

### Modes and safety

`/deep` allows looking inside the original; `/clean` returns to clean-room. `/yolo` approves everything without asking (`/yolo off` to end it). See [Clean-room and deep mode](modes.html) and [Permissions](permissions.html).

### Settings

`/team on|off`, `/venv on|off`, `/theme light|dark`, `/thinking` (show or hide reasoning), `/sidebar`. `/team` and `/venv` change the running session; put them in `portcode.json` to keep them. See [Configuration](config.html).

### Sessions

`/compact` (or `/summarize`) summarises the conversation to free context. `/new` or `/clear` start a fresh session. `/resume` and `/sessions` open the session list. See [Sessions](sessions.html).

### Work

`/create` starts the guided flow. `/workbench [binary]` opens the deep code view. `/ledger` shows the evidence ledger. `/engines` lists the reverse-engineering engines. `/tools` lists the tools. `/agent <name>` and `/agents` switch and list agents. `/update` checks for a new release. `/remote` starts remote control.

### Slash commands and remote control

A remote client cannot send slash commands as text: the browser page has its own controls for the agent, the mode and cancelling. This is deliberate, see [Remote control](remote.html).
