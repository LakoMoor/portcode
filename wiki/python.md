# Python environments

For Python work the agent keeps everything in a virtual environment inside the project, never in your global Python.

## What happens

1. The agent calls the `venv` tool with `action: "create"`. PortCode runs `python3 -m venv .venv` in the project (after you approve it).
2. From then on, every `bash` command runs with `VIRTUAL_ENV` set and `.venv/bin` first on `PATH`, so `python`, `pip` and `pytest` mean the project's own.
3. The agent installs packages with `venv` and `action: "install"`, which runs `pip install` inside `.venv`.

The tool has three actions:

| Action | Does |
|---|---|
| `status` (default) | says whether `.venv` exists and which Python it has |
| `create` | creates `.venv`; does nothing if it exists |
| `install` | installs the listed packages into `.venv` |

```json
{"action": "install", "packages": ["numpy", "pytest>=8"]}
```

## Safe by construction

- Package names must be plain requirements (`pytest>=8`, `requests[socks]==2.32.0`). Flags, URLs and anything with shell characters are refused, so the model cannot pass `--index-url` or a git URL.
- Creating the environment and installing both ask for permission.
- The project's API-key variables are removed from the environment of these commands.

## Turning it off

It is on by default. Set `"venv": false` in `portcode.json`, or use `/venv off` for the running session (`/venv on` to bring it back). When it is off, `create` explains that and `bash` keeps your normal `PATH`.

If creation fails, the error says whether Python 3 or, on Debian and Ubuntu, `python3-venv` is missing.
