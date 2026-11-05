# A tour of the app

The interface is a full-screen terminal app in the style of OpenCode. `portcode chat --plain` gives a plain line-by-line mode instead.

## The home screen

When a session is empty you see the logo, the prompt box and a row of hints. The prompt box has a coloured bar on its left edge: the colour is the active agent. Under it you see the model, the provider and the mode (`▣ clean-room`, or a yellow `◉ deep`).

## The first-message suggestion

In a fresh session, when you have not typed anything, PortCode reads the folder and shows a grey, italic guess at what you want to do first, for example *Read this Rust project, run its tests, and tell me what is unfinished or broken.* The box tells you the keys: `tab` uses it, `esc` dismisses it, typing replaces it.

How it works:

1. PortCode lists the top level of the folder (and the first entries of each subfolder) and reads the first lines of a **fixed list** of project files: `README*`, `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `Makefile`, `CMakeLists.txt` and `docs/SPEC.md`. It never reads `.env`, keys or any other file.
2. It asks the model you already chose for one message you might send, in the language of your README.
3. If the provider needs a key and has none, if you use a subscription backend, or if the model does not answer within 15 seconds, it falls back to a local guess based on what is in the folder (an `original/` folder, a `Cargo.toml`, and so on).

> [!NOTE]
> Step 2 sends the folder listing and those file heads to your configured provider. Set `"suggest": false` in `portcode.json` to turn the whole feature off.

## The transcript

Your messages, the agent's answers and every tool call appear in order. A tool call is one line with an icon (a spinner while it runs, `✓` or `✗` after), the tool name and a short summary; the first lines of its output hang under it. File edits show as coloured diffs. A `parity` run ends with a green **PASS** or red **FAIL** banner.

Evidence ids such as `E-0003` and unknown ids are coloured chips so you can follow them.

When the agent cannot go on without you, it stops with a yellow **Needs your decision** card that shows the whole question. Answer in the prompt and it continues.

Press `ctrl+t` to show or hide the model's reasoning. Press `pgup` / `pgdn` to scroll.

## The prompt

- `enter` sends, `alt+enter` adds a new line.
- `↑` and `↓` walk your history, and bring back your draft when you reach the end.
- Type `/` for the command menu: `↑` `↓` to choose, `tab` or `enter` to complete.
- While a request runs, press `esc` twice to cancel it. The footer says *press esc again to cancel* after the first press.

## The sidebar

On terminals at least 110 columns wide the right-hand side shows the session: title, context use, the todo list, the evidence ledger counts (observed, inferred, unknown) and the last parity result. `ctrl+b` hides it.

## Permission dialog

A tool that needs approval pauses the run and asks. Press `y` to allow once, `a` to always allow that exact request for the rest of the session, `n` to reject. If several workers ask at once (team mode), the requests queue up. See [Permissions](permissions.html).

## Screens beyond the chat

| Open with | What it is |
|---|---|
| `/create` | a guided start: what to recreate, how closely to look, what to build on |
| `/workbench` | functions, Pseudo-C, disassembly, hex and the evidence log side by side (deep mode) |
| `/resume` | the list of this project's saved sessions, see [Sessions](sessions.html) |
| `/ledger` | the evidence ledger |
| `/remote` | remote control from a browser or phone, see [Remote control](remote.html) |

## Themes

`/theme light` and `/theme dark` switch the colours and are remembered. `PORTCODE_THEME` overrides the saved choice.
