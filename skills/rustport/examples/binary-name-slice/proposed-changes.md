# Proposed rustport guidance changes

No installed skills were edited.

1. Replace the binary-only workflow's unconditional “build the original” and source-module reading steps with artifact identity, focused decompilation, and isolated runtime observation. This test has a shipped executable and expressly forbids source access.
2. Document that HOME and XDG overrides plus fresh working/output directories reduce data exposure but are not an OS sandbox. Keep copies, process-group deadlines, minimal environments, and a record of actual launch diagnostics. macOS native services may remain unavailable despite matching architecture.
3. Make static-only reconstruction an explicit milestone: static expectation tests must be labeled inferred, with original differential tests skipped rather than passed when the runtime oracle is unavailable. Oracle-before-code cannot be satisfied by decompiler-derived expected values.
4. Recommend projecting binary_session to provider binding/activity/health before displaying it. This server's complete session response includes the full tool catalog and is extremely large; printing it obscures useful status.
5. A long-analysis policy must account for the MCP client transport deadline. In this run the sole Ghidra request was forcibly timed out after 300 seconds and its temporary project disappeared despite a requested 15-minute allowance. Recommend detecting/documenting the deadline before costly imports and reporting cleanup explicitly; do not imply a status poll can prolong that request. No config changes were attempted here.
6. `binary_session` returned `not_observed` activity and null operation health while the current Ghidra log advanced, then after its transport failure. Recommend documenting this observability limitation and preserving current-run logs before transport cleanup; do not interpret empty activity as idle.
