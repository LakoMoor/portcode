# Deep engines (deep mode only)
Look inside only in deep mode and only for software you own or may examine. Check `re_status` first; install with `re_install` (asks) or `portcode engines install <id>`. Everything lands in the `engines/` folder next to the binary.

| Question | Tool / engine | Notes |
|---|---|---|
| What is this file? Packed? | `hexdump` mode=entropy / strings; `die`, `binwalk` | entropy > 7.5 = packed or encrypted |
| What does this code do? | `disasm` (Capstone) | file offset, not virtual address; set `base` |
| What is the whole function? | `decompile` (Kuna) | list → func; no project, each call is independent |
| What does it compute for input X? | `emulate` (Unicorn) | set regs, run `start`..`end`, read regs/memory |
| Unity game? Mono or IL2CPP? | `unity` (survey), then `dotnet` / `unity_assets` | Mono: C# in `Managed/*.dll`; IL2CPP: Il2CppDumper first, then `dotnet` on its DummyDll |
| C# / .NET assembly (Unity or not) | `dotnet` mode=types → type → strings → refs; `decompile` via ILSpy | obfuscated names (a, b) are normal: strings and P/Invoke refs orient you |
| Scenes, TextAssets, MonoBehaviour values | `unity_assets` mode=list → text / tree | needs the type tree; stripped games need the assembly too |
| Binary file format? | ImHex `.hexpat` + `lief` | write the pattern in `docs/formats/`, open it in ImHex |
| Symbolic / path questions | `angr` | heavy; last resort |
| Live process | `frida` | only with approval to run the original |

Rules
1. Deep output is `inference`. Confirm with a golden from the running original.
2. Cite tool and address in every ledger entry.
3. Rewrite in your own words and structure. Never paste decompiler output or disassembly into the port, docs or site.
4. Emulate small pure functions; a missing syscall or import stops the emulator, which is an `unknown`, not a failure.
