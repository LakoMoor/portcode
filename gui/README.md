# PortCode desktop app

The same agent as the terminal app, in a window built on [GPUI Fast](https://github.com/longbridge/gpui-fast):
a transcript with foldable tool calls, a composer with agent / deep / allow-all chips, and a ledger sidebar with
context use, evidence (seen / inferred / open), the parity verdict and the agent's checklist.

It is a separate workspace on purpose: the terminal build never needs a graphics toolchain.

## Build and run

```sh
cd gui
cargo run --release            # opens in the current folder
cargo run --release -- ~/proj  # or in another folder; add --deep for deep mode
```

Needs Rust 1.98 (`rust-toolchain.toml` picks it) and, on Linux, the Wayland/X11 development packages GPUI lists.
macOS compiles its shaders at run time, so Xcode is not required.

`portcode gui` starts it from the terminal when `portcode-gui` sits next to `portcode`.

## Keys

| Key | Does |
|---|---|
| Enter / Shift+Enter | send / new line |
| Esc | interrupt the agent |
| type while it works | steers the agent: it reads your message before its next step |
| Cmd+\ (Ctrl+\) | show or hide the ledger |
| in a permission question: y / a / A / n | allow once / always / allow all / reject |

Slash commands: `/deep` `/clean` `/yolo` `/clear` `/compact` `/agent <name>` `/model <provider/model>` `/context 32k` `/theme` `/sidebar`.

## Not done yet

Text selection and copying from the transcript, a project picker (it opens the folder you start it in), Linux and
Windows builds in the release, `/connect` and `/create` screens.

`PORTCODE_GUI_DEMO=1` opens a made-up session without a model, for screenshots.
