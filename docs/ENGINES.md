# Reverse-engineering engines and modes

PortCode has two modes.

| | clean-room (default) | deep |
|---|---|---|
| Looks at | what a user sees: run it, inputs, outputs, screenshots, public docs | also inside: disassembly, decompilation, emulation, raw bytes |
| Tools | `bash`, `project_check`, `parity`, `screenshot`, `survey`, ledger | + `disasm`, `emulate`, `decompile`, `hexdump`, `unity`, `cpp2il`, `dotnet`, `unity_assets`, `re_install` |
| Refused | `disasm` `emulate` `decompile` `hexdump` `cpp2il`; objdump/strings/otool/gdb/r2… in `bash` | nothing extra |
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
| kuna | Rust decompiler (a port of Ghidra's), one binary | `decompile` | official release archive, sha256 from GitHub; no JDK |
| cpp2il | Unity IL2CPP managed assembly and method-body recovery | `cpp2il` | official self-contained binary, sha256 from GitHub |
| imhex | hex editor + pattern language | you (agent writes `.hexpat`) | cask / AppImage |
| rizin, radare2, binwalk, die, capa, jadx, wabt, upx | system tools | `bash` (deep) | your package manager |

```sh
portcode engines install core        # capstone + unicorn + lief
portcode engines install cpp2il kuna imhex
portcode --deep -a reverse
```
Emulation never touches the host: no syscalls, a hard instruction budget, a 60 s deadline.
Use only on software you own or may examine. PortCode never bypasses DRM, logins or anti-cheat.

## Workbench

`/workbench [file]` (deep mode only; with no file it takes the first executable in `original/`) opens one screen for reading a binary:

| area | shows | from |
|---|---|---|
| left | functions: address, size, name; `/` filters | `functions` (LIEF) |
| middle | **1** Pseudo-C · **2** Disassembly · **3** Hex of the selected function | `decompile` (Kuna), `disasm` (Capstone), `hexdump` |
| right | the evidence ledger, newest first; records that mention the selected function are highlighted | `.portcode/evidence` |
| bottom | the agent's last answer and the prompt | the current session |

Keys: `j`/`k` move, `1 2 3` or `h`/`l` switch view, `PgUp`/`PgDn` scroll, `/` filter, `a` ask the agent to explain the function and record it as an inference, `i` type to the agent, `r` reload, `q` close. The first Pseudo-C of a binary includes Ghidra's analysis, so it can take a while; later functions reuse the project. Sizes are the distance to the next symbol, so treat them as estimates. The workbench makes the same tool calls an agent would, and it refuses to open in clean-room mode.

## Unity and C# / .NET

| tool | does | needs |
|---|---|---|
| `unity` | surveys a game folder or `.app`: Unity version, **Mono or IL2CPP**, managed DLLs, native library, asset and bundle counts, next steps | nothing |
| `cpp2il` | recovers managed DLLs from an IL2CPP game folder or APK; `types` restores signatures, `methods` also attempts IL body recovery | `cpp2il` official binary |
| `dotnet` | `info`, `types`, `type`, `strings`, `refs` (assembly refs and P/Invoke) of any .NET assembly, including Unity's `Assembly-CSharp.dll` | `dnfile` (pip) |
| `dotnet` mode=`decompile` | C# source of one type | `ilspy` (`dotnet tool install -g ilspycmd`, needs the .NET SDK) |
| `unity_assets` | `info`, `list`, `text`, `tree` of `.assets`, `levelN`, `globalgamemanagers`, bundles: scenes, TextAssets, MonoBehaviour values | `unitypy` (pip) |

Install: `portcode engines install cpp2il dnfile unitypy`. Mono games keep their C# in `Managed/*.dll`, so `dotnet` reads it directly. IL2CPP games compile C# to native code, so run Cpp2IL first:

```sh
portcode --deep tool cpp2il '{"path":"original/MyGame","mode":"types"}'
portcode --deep tool cpp2il '{"path":"original/MyGame","mode":"methods","output":"recovered-methods"}'
portcode --deep tool dotnet '{"file":".portcode/analysis/cpp2il-types/Assembly-CSharp.dll","mode":"info"}'
```

`types` is the faster first pass. `methods` uses Cpp2IL's IL recovery and can be substantially slower. `path` is the game install folder (not `GameAssembly.dll`) or an APK; set `exe_name` only when executable auto-detection is ambiguous. Output goes to a new or empty folder, by default `.portcode/analysis/cpp2il-<mode>`, with the complete `portcode-cpp2il.log`; PortCode refuses to overwrite recovered evidence. The installer chooses a recent official release asset with a GitHub-published SHA-256 rather than installing an unverified latest stable binary. On macOS it ad-hoc signs the already verified binary so Gatekeeper can execute the unsigned upstream release.

The recovered DLLs are analysis artifacts, **not the original C# and not a runnable Unity project**. IL recovery is imperfect and unsupported metadata or obfuscation can leave gaps. Continue with `dotnet`, `unity_assets`, and native `functions`/`decompile`/`disasm`, then build a separate port and prove its behaviour with `project_check` and golden-based `parity`. Record recovered facts as `inference`, not observation or proof of parity.
