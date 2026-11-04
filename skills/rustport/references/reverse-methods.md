# Choosing a reverse-engineering route by original technology

Use the original executable, source, bytecode, or script as the specification. Record version/build SHA, exact observations, uncertainties and confidence. Decompiled pseudo-C alone is an inference, not a runtime test.

| Original | Preferred primary evidence | Additional tooling | Beware |
|---|---|---|---|
| C/C++ native, Delphi, Windows EXE | File formats, isolated gameplay traces and static decompilation | REA, Ghidra headless, objdump, x64dbg/WinDbg, Unicorn for controlled narrow routines | 32/64-bit differences, ABI, floating point, packed resources, license terms |
| Java bytecode/JAR | Bytecode + decompiler + original-JVM oracle | CFR/FernFlower, javap, ASM, Java test harness | JVM float/overflow/Math/Random, class loader, reflection |
| Source / Garry's Mod Lua | Lua scripts, BSP/MDL/VTF/VPK and actual game callbacks | Lua parser, game console test harness, Valve tooling | Tick order, hooks, prediction, engine native functions, client/server split |
| Unity managed/IL2CPP | C# assemblies or metadata + runtime behaviors | dnSpyEx, Il2CppInspector, REA | Serialized type IDs, runtime version, native engine contracts |
| Unreal / C++ | Assets, reflection data, runtime tests, native code | UnrealPak, FModel, Ghidra where appropriate | Cooked assets, engine version and licensing |
| Web / Electron | Source maps, JS bundle, DevTools traces | REA browser/electron, Playwright | Browser event ordering, storage and network externalities |
| Existing open-source port | Original regression tests + call graph + original content | Rust compiler, trace diff, wgpu capture | Check active runtime wiring before duplicate work |

## An evidence-first loop

Inventory → classify → observe → pin → trace one function/format → document constants, event order and I/O → implement Rust → run differential test → inspect first mismatch → integrate into *real* game loop → audit ledger.

For a PE function suitable for emulation, optionally test via Unicorn with modeled memory, imports and exception behavior. An incorrectly modeled process/environment produces misleading results. When the original is interactive, define an automation-compatible fixture (fixed seed/timestep and explicit inputs) rather than assuming the emulator alone can prove the whole gameplay feature.

For graphics, use renderdoc-equivalent capture when possible and explicit offscreen PNG/reference metadata. Compare color space, culling, texture coordinates, LOD, blending, depth, shadows, gamma and driver/platform differences; do not infer exact 1:1 parity from visually similar screenshots.

For format compatibility, validate samples from *real installations*: file counts, parse coverage, trailing bytes, referenced dependencies, round-trip stability and error behavior. Keep proprietary data in an ignored local directory.
