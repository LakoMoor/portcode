# Agents

An agent is a role: a system prompt, a set of tools, a step limit and a permission stance. Switch with `tab` and `shift+tab`, with `/agent <name>`, or start with `portcode -a <name>`. `portcode agents` lists them all.

## The agents you switch between

| Agent | Use it to | Tools | Steps |
|---|---|---|---|
| `build` | do general work: implement, fix, refactor. The default. | all | 60 |
| `port` | port a program to Rust with evidence, goldens, screenshots and a site (the *rustport* workflow) | all | 150 |
| `reverse` | study a binary, app or file format and record evidence. Does not change code. | the deep tools, read tools | 80 |
| `plan` | investigate and produce a plan. Read-only: edits are denied and nothing prompts. | read tools | 40 |
| `team` | lead a team of workers in parallel. Only listed when team mode is on. | read tools, `bash`, `team` | 80 |

### build

The everyday agent. It reads before it edits, makes small changes, runs your project's build or tests, and reads the first error before fixing.

### port

`port` follows the rustport skill phase by phase and does not stop early. It has a **finish gate**: if a behaviour of the original is not in the todo list, a todo is open, or the last `parity` run did not pass on the final code, the run is sent back to work and given more steps. The only way out is `blocked`, when it needs a decision only you can give. See [The port workflow](port-workflow.html).

### reverse

`reverse` studies an original and records what it learns. Run it with `--deep` to let it use `disasm`, `emulate`, `decompile`, `hexdump` and the Unity and .NET tools. A strict two-session setup works well: a `reverse --deep` session writes `docs/SPEC.md` in its own words, then a `port --clean-room` session builds the port from that spec alone. See [Clean-room and deep mode](modes.html).

### plan

Use `plan` when you want a plan and no changes. It cannot edit, and it never asks for permission.

### team

Orchestrator for [Team mode](team.html).

## Sub-agents

Sub-agents run inside another agent's turn with a fresh context. You do not switch to them. They are reached with the `task` tool (`explore`, `general`) and the `team` tool (the roles).

| Sub-agent | Job | Edits files |
|---|---|---|
| `explore` | fast read-only search and summaries | no |
| `general` | a self-contained job | yes |
| `pm` | turn a goal into steps and done-criteria | no |
| `techlead` | design the work and assign files to developers | no |
| `researcher` | find facts with file:line evidence | no |
| `developer` | implement an assigned block of files | yes |
| `tester` | run and write tests | yes (tests) |
| `reviewer` | review changes for bugs and slips | no |

A sub-agent sees nothing of your conversation, so the task you give it must be complete.

## Per-agent settings

In `portcode.json`, `"agent": { "build": { "model": "ollama/qwen3:14b", "steps": 80, "prompt_append": "Always run cargo clippy." } }` gives an agent its own model, step limit or extra prompt text. See [Configuration](config.html).
