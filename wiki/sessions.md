# Sessions and history

Every conversation is saved in your project, in `.portcode/sessions/` as one file per session. You can leave and come back to it later.

## Resuming

| Command | What it does |
|---|---|
| `portcode --resume` (or `-r`) | opens a list of this project's sessions to pick from |
| `portcode --resume 2` | opens session number 2 of that list |
| `portcode --resume s-1762300000` | opens a session by id; a unique prefix of an id is enough |
| `portcode -c` | continues the latest session (`chat -c`, `run -c` too) |
| `/resume` or `/sessions` | in the app: the same list |
| `/resume 2`, `/resume last` | in the app: open directly |

The list shows, newest first, each session's first message as its title, how long ago it was used and how many messages it holds. Pick with `↑` `↓` and `enter`; `esc` starts a new session.

When a session opens, the old transcript is drawn again, including tool calls. The model's reasoning is not kept on screen.

`portcode run` has no screen, so `--resume` without a value is refused there: give a number or id, or use `-c`.

## Managing sessions from the shell

```sh
portcode sessions            # list, newest first
portcode sessions show 2     # print the conversation
portcode sessions rm 2       # delete one (not the one in use)
```

## New sessions

`/new` and `/clear` start a fresh session. The old one stays on disk.

## Freeing context with `/compact`

`/compact` (alias `/summarize`) asks the model to write a dense summary of the whole conversation and replaces the history with it. The summary keeps the goal, decisions, files changed, commands run and their results, evidence ids and the exact next step. Use it when a long session nears the model's context window, or before changing direction. The sidebar's context figure drops, and you carry on in the same session.

- PortCode also compacts automatically when the window is nearly full; `/compact` just lets you do it earlier.
- The summary uses `small_model` if you set one, otherwise the current model.
- It is not available while a request is running, and the Claude Code and Codex backends manage their own context (use their own `/compact`).

## Good to know

- Sessions belong to the folder you ran PortCode in. Another folder has its own list.
- With the Claude Code or Codex backends, a resumed session starts a new conversation inside that CLI, so the CLI itself does not remember the earlier turns. PortCode's own record and the on-screen transcript are restored.
- Long sessions are compacted automatically when they near the model's context window; the summary keeps the goal, decisions, files changed and evidence ids. `small_model` in the config picks a cheaper model for that.
