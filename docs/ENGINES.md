# Reverse-engineering engines and modes

PortCode has two modes.

| | clean-room (default) | deep |
|---|---|---|
| Looks at | what a user sees: run it, inputs, outputs, screenshots, public docs | also inside: disassembly, decompilation, emulation, raw bytes |
| Tools | `bash`, `parity`, `screenshot`, `survey`, ledger | + `disasm`, `emulate`, `decompile`, `hexdump`, `re_install` |
| Refused | `disasm` `emulate` `decompile` `hexdump`; objdump/strings/otool/gdb/r2… in `bash` | nothing extra |
| Evidence | mostly `observation` | mostly `inference` (cite tool + address) |
| Switch | `--clean-room` | `--deep`, `/deep`, `"mode":"deep"`, `PORTCODE_MODE=deep` |

Clean-room works the way [craft-anything](https://github.com/storytold/craft-anything) does: tools that would look inside are refused and the refusal explains how to enable deep mode. PortCode adds a **two-session hand-off**: a `reverse --deep` session writes `docs/SPEC.md` and ledger entries in its own words; a `port --clean-room` session builds from those only.

## Engines
`portcode engines list` shows what is installed. Everything PortCode installs goes into the `engines/` folder next to the `portcode` binary (override: `PORTCODE_ENGINES`). Installs ask first and downloads are sha256-verified.

| id | what | used by | install |
|---|---|---|---|
| capstone | disassembler (x86, ARM, ARM64, MIPS, RISC-V, PPC) | `disasm` | pip, private venv |
| unicorn | CPU emulator | `emulate` | pip, private venv |
| keystone | assembler | bash | pip |
| lief | ELF/PE/Mach-O/DEX parser | bash/python | pip |
| yara | file pattern rules | bash/python | pip |
| angr | symbolic execution | python | pip (large) |
| frida | dynamic instrumentation | bash | pip |
| ghidra | decompiler, run headless | `decompile` | official release zip, sha256 from GitHub; needs a JDK |
| imhex | hex editor + pattern language | you (agent writes `.hexpat`) | cask / AppImage |
| rizin, radare2, binwalk, die, capa, jadx, wabt, upx | system tools | `bash` (deep) | your package manager |

```sh
portcode engines install core        # capstone + unicorn + lief
portcode engines install ghidra imhex
portcode --deep -a reverse
```
Emulation never touches the host: no syscalls, a hard instruction budget, a 60 s deadline.
Use only on software you own or may examine. PortCode never bypasses DRM, logins or anti-cheat.

## Workbench

`/workbench [file]` (deep mode only; with no file it takes the first executable in `original/`) opens one screen for reading a binary:

| area | shows | from |
|---|---|---|
| left | functions: address, size, name; `/` filters | `functions` (LIEF) |
| middle | **1** Pseudo-C · **2** Disassembly · **3** Hex of the selected function | `decompile` (Ghidra), `disasm` (Capstone), `hexdump` |
| right | the evidence ledger, newest first; records that mention the selected function are highlighted | `.portcode/evidence` |
| bottom | the agent's last answer and the prompt | the current session |

Keys: `j`/`k` move, `1 2 3` or `h`/`l` switch view, `PgUp`/`PgDn` scroll, `/` filter, `a` ask the agent to explain the function and record it as an inference, `i` type to the agent, `r` reload, `q` close. The first Pseudo-C of a binary includes Ghidra's analysis, so it can take a while; later functions reuse the project. Sizes are the distance to the next symbol, so treat them as estimates. The workbench makes the same tool calls an agent would, and it refuses to open in clean-room mode.
