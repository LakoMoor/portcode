# First run

Five minutes from install to your first task.

## 1. Open a folder

Run `portcode` in the folder you want to work in. For a port, that is the folder that holds the program to port: put the original in `original/`.

When the prompt is empty, PortCode looks at the folder and puts a guess at your first message in the prompt box as grey text. Press `tab` to use it, `esc` to dismiss it, or just type something else. See [A tour of the app](tour.html#the-first-message-suggestion).

## 2. Pick a model and give it a key

PortCode speaks the OpenAI chat protocol, so almost any server works. Choose one way:

- **A local model, no key.** Start Ollama, LM Studio, llama.cpp or vLLM, then type `/provider ollama qwen3:14b`.
- **Your Claude or ChatGPT subscription.** Install Claude Code or Codex, sign in there once, then `/provider claude-code sonnet` or `/provider codex default`. No key and no API bill. See [Claude Code and Codex](subscriptions.html).
- **A hosted API.** Type `/key` and paste the key. It is hidden while you type and saved in your config folder with mode 600. From a shell: `portcode auth set <provider>`, or `portcode auth set <provider> --from-env NAME` to read it from an environment variable.

| Type | What it does |
|---|---|
| `/provider` | lists providers and shows which still need a key |
| `/provider <name> [model]` | switches provider and model |
| `/key [provider]` | asks for the API key, hidden |
| `/models` | lists the models the current provider serves |
| `/model <provider>/<model>` | picks a model |
| `/setup` | prints these steps again |

The model you choose is remembered for the next start. `-m` and `PORTCODE_MODEL` still win.

## 3. Check that it works

```sh
portcode doctor
```

To test the model itself (chat, tool calls, valid JSON, edits) run `portcode eval`.

## 4. Start working

Type a task, or type `/create`. `/create` asks what you want to recreate (a website, a program, a game, a file format or nothing yet), how closely PortCode may look inside the original, and what to build it with. Then it starts the right agent in the right mode.

```text
Port this CLI to Rust, following rustport
```

PortCode pins the original's sha256, records what it learns, and calls a module done only after `parity` passes. The details are in [The port workflow](port-workflow.html).

## Settings worth knowing early

| Setting | Default | What it does |
|---|---|---|
| `esc` twice | | cancels the running request |
| `/yolo` | off | approves every permission request without asking, see [Permissions](permissions.html) |
| `"venv"` | on | lets the agent create and use a Python `.venv`, see [Python environments](python.html) |
| `"team"` | off | adds the `team` agent, see [Team mode](team.html) |
| `"suggest"` | on | the grey first-message suggestion |
