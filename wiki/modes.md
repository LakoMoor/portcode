# Clean-room and deep mode

The mode decides how closely PortCode may examine the original.

| | Clean-room (default) | Deep |
|---|---|---|
| Observe from outside: run it, feed inputs, compare outputs | yes | yes |
| Read its documentation and public files | yes | yes |
| Disassemble, decompile, emulate, hexdump | no | yes |
| Inspect Unity, .NET and IL2CPP internals | no | yes |
| Chip in the app | `▣ clean-room` | `◉ deep` |

## Switching

```sh
portcode --deep          # start in deep mode
portcode --clean-room    # explicitly clean-room (the default)
```

In the app: `/deep` and `/clean`. You can also set `"mode": "deep"` in `portcode.json`.

## What is enforced

The deep tools (`disasm`, `emulate`, `decompile`, `functions`, `hexdump`, `unity`, `dotnet`, `unity_assets`) refuse to run in clean-room mode and say how to switch. `bash` also refuses commands that look inside programs (disassemblers, decompilers, debuggers, and the like) in clean-room mode.

> [!NOTE]
> The guard is a seatbelt, not a sandbox. Use deep mode only on software you own or may examine.

## The deep tools

| Tool | For |
|---|---|
| `disasm` | disassembly with Capstone (x86, ARM, ARM64, MIPS, RISC-V, PPC…) |
| `emulate` | run a function or decoder in isolation with Unicorn |
| `decompile` | Pseudo-C from Kuna (one downloaded binary, no JDK); also function lists, strings and cross-references |
| `functions` | list the functions of a binary |
| `hexdump` | bytes at an address |
| `unity` | a Unity project, Mono or IL2CPP |
| `dotnet` | types, methods, strings and decompilation of C# and .NET (ILSpy) |
| `unity_assets` | scenes, TextAssets and MonoBehaviour data |

Engines install on request, after you approve, into an `engines/` folder next to the binary, and every download has a published sha256 that is checked. See [Engines](engines.html).

## The workbench

`/workbench` (deep mode) shows functions, Pseudo-C, disassembly, hex and the evidence log side by side. `j` and `k` move, `1` `2` `3` switch the view, `/` filters, and `a` asks the agent about the selected function.

## The strict two-session method

1. `portcode --deep -a reverse`: study the original and write `docs/SPEC.md` in your own words.
2. `portcode --clean-room -a port`: build the port from that spec only.

The second session never sees the original's internals, which is what "clean room" means.
