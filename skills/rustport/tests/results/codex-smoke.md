# Codex CLI integration smoke test

Date: 2026-10-29. Client: bundled Codex CLI `0.165.0-alpha.1.2`.

Installed with `python3 install.py --agent codex --root <temporary-project>`.
Executed a read-only `codex exec --ephemeral --ignore-user-config
--skip-git-repo-check --sandbox read-only` request prefixed with `$rustport`.
The request asked for a compatibility plan for a simulator using original maps
and vehicle scripts, with no implementation, installation or network research.

Observed:

- Codex announced that it was using rustport.
- It resolved `.agents/skills/rustport/SKILL.md` in the temporary project and read
  that file plus `references/compatible-runtime.md` using its shell tool.
- It selected compatible-runtime mode.
- It proposed loading an original vehicle and map section, executing scripts,
  starting the engine and driving through the ordinary runtime entrypoint.
- It distinguished implementation, runtime wiring, real-content verification and
  differential verification, and did not treat missing comparisons as passes.
- The turn completed successfully (process exit 0).

This verifies project skill discovery, `$rustport` invocation, reference routing
and a narrow instruction-following scenario. It does not test the interactive
`/skills` selector, a complete game port, GPU execution, or the other five hosts.

The npm `codex` launcher on this machine failed before startup because its vendor
binary was missing (ENOENT); the bundled client above was used without changing
that installation. No credentials or raw session logs are included here.

## Full source-port follow-up

Codex then completed the dependency-free C-to-Rust example in
`examples/codex-cli-port/`: it compiled the original and captured 13 goldens before
writing Rust, compared save/load in both directions, ran 120 seeded traces and
parser edge cases, and passed offline build/fmt/clippy checks. A separate parent
check compared another 200 traces of 1,000 commands each byte-for-byte.
Performance timings varied; no speed or whole-game compatibility claim is made.
The final process exited successfully. Interactive `/skills` remains untested.
