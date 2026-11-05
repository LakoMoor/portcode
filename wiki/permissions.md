# Permissions and allow-all

PortCode keeps you in charge. A tool that changes something asks first, unless a rule says otherwise.

## What asks by default

File edits and shell commands ask. Reading, listing and searching files do not. Fetching a web page asks. Every question names the tool and the exact target (the command, the path).

Read-only agents (`plan`, and the read-only sub-agents) never ask: anything that would need approval is simply denied, and the model is told.

## Answering

| Key | Reply |
|---|---|
| `y` or `enter` | allow once |
| `a` | always allow that exact request for the rest of the session |
| `n` or `esc` | reject |

When several workers ask at once in [team mode](team.html), the dialogs queue and you answer them one after another.

## Rules in `portcode.json`

```json
{
  "permission": {
    "edit": "ask",
    "bash": { "*": "ask", "cargo test*": "allow", "git push*": "deny" },
    "webfetch": "deny"
  }
}
```

A rule is `allow`, `ask` or `deny`. For tools that take a target (`bash`, `edit`, `write`), you can give a map of patterns; `*` matches anything. A `deny` rule hides the tool from the model entirely when it applies to the whole tool.

## Folders outside the project

File tools stay inside the project folder. To let them touch other folders without asking, list them in `"allowed_dirs"`.

## Allow-all

`/yolo` approves every permission request without asking. While it is on, a red **⚠ allow-all** chip stays on screen. `/yolo off` ends it, and so does a new run of PortCode: it is never saved.

- Rules that say `deny` still deny.
- With Claude Code as the backend, allow-all sets its permission mode to `bypassPermissions`; turning it off returns to `acceptEdits`.
- From a script, `portcode run -y "task"` does the same for one run.

> [!WARNING]
> Allow-all lets the agent run any shell command and edit any file in the project without asking. Use it only in a project and on a machine you trust, ideally in a throwaway folder or a container.

## Clean-room guard

Separately from permissions, `bash` refuses tools that look inside programs (disassemblers, decompilers, debuggers) while the session is in clean-room mode. See [Clean-room and deep mode](modes.html).

## Secrets

Commands the agent runs do not inherit your API-key environment variables (the ones your providers use are removed), and the agent is told never to print or store secrets.
