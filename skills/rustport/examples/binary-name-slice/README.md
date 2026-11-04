# Binary-derived VPD name validation experiment

Static-inferred reimplementation from the official openMMD v0.1.0-preview.2
macOS arm64 executable. Original application source was not supplied or consulted.
This is a bounded helper, not an editor/engine reconstruction or measured parity.

Executable SHA256: `21f70318184847974e7b8de535f170761c1ad97d2540b4bc29abf5f0e1c7f882`.
Assembly anchor: `0x100425ca8..0x100426160`. Disassembly is retained under evidence/.
The original MIT notice is in evidence/ORIGINAL-LICENSE.

Run `cargo test --offline --manifest-path port/Cargo.toml`: 3 static expectation
tests pass; 1 original-side differential test is ignored because no runtime oracle
was captured. Formatting, check and Clippy also passed in the test run.

REA MCP opened the artifact and inspected Mach-O metadata (Evidence ID
`ev_655a98751510098c48b0e7d1b09d665b76f81ea90799e008010ea631241a9b4a`).
Ghidra 12.1.4 showed current-run decompilation activity, but its request hit the
300-second transport deadline and lost its temporary database. No pseudocode
result was obtained. The helper was recovered separately from arm64 disassembly.

Original native launch and speculative --help probes used executable copies,
fresh HOME/config/cache/output paths, and process-group deadlines. Sandbox runs
reported macOS service errors; the parent repeated the same probes outside that
sandbox and obtained no stdout/stderr before the 8-second deadlines. Neither run
provided feature outputs or application compatibility evidence. A GUI application
remaining alive is not itself failure or proof that --help is supported.

Unknown: original caller roles and full Unicode behavior; no original-side
comparison, full VPD parsing, graphics, animation or whole-editor compatibility.

## License

Rustport additions are Apache-2.0; the retained upstream assembly material and
applicable openMMD attribution remain covered by the original MIT notice in
[evidence/ORIGINAL-LICENSE](evidence/ORIGINAL-LICENSE).
