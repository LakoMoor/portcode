# Keyboard shortcuts

## In the prompt

| Keys | Action |
|---|---|
| `enter` | send |
| `alt+enter` | new line |
| `tab` | use the start-up suggestion, or complete a `/` command, or switch to the next agent |
| `shift+tab` | switch to the previous agent |
| `↑` / `↓` | walk the prompt history (or move in the `/` menu) |
| `ctrl+a` / `ctrl+e` (or `home` / `end`) | start / end of the line |
| `ctrl+u` | delete from the cursor to the start of the line |
| `ctrl+w` | delete the word before the cursor |
| `ctrl+c` | clear the prompt if it has text, otherwise quit |
| `ctrl+d` | quit when the prompt is empty |
| `esc` | dismiss the suggestion, close a menu, or scroll back to the bottom |

## While a request runs

| Keys | Action |
|---|---|
| `esc` `esc` | cancel the request (the footer says *press esc again* after the first) |
| `ctrl+c` | cancel the request at once |

## View

| Keys | Action |
|---|---|
| `ctrl+t` | show or hide the model's reasoning |
| `ctrl+b` | show or hide the sidebar |
| `pgup` / `pgdn` | scroll the transcript |
| mouse wheel | scroll the transcript |

## Permission dialog

| Key | Reply |
|---|---|
| `y` or `enter` | allow once |
| `a` | always allow that exact request for the rest of the session |
| `n` or `esc` | reject |

## Session list (`--resume`, `/resume`)

| Key | Action |
|---|---|
| `↑` `↓` or `k` `j` | move |
| `pgup` `pgdn` `home` `end` | jump |
| `enter` | open the session |
| `esc` or `q` | leave without opening one |

## Workbench (`/workbench`)

| Key | Action |
|---|---|
| `j` / `k` | move in the function list |
| `1` `2` `3` | Pseudo-C, disassembly, hex |
| `/` | filter functions |
| `a` | ask the agent about the selected function |
| `esc` | clear the filter, then close |
