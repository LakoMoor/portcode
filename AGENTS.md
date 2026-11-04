# AGENTS.md — guide for agents and contributors

PortCode: Rust agent for reverse engineering and ports, built for local/self-hosted LLMs. See `docs/ARCHITECTURE.md`.

- Layers: evidence(L0) → port, llm(L1) → agent(L2) → cli(L3). Lower layers never import higher ones.
- `unsafe` forbidden. No `unwrap`/`expect`/indexing on input-derived values outside tests (lints enforce it in L0–L2 crates).
- Everything user-visible is a `Tool` (`crates/portcode-agent/src/tools`); the CLI, agent and `portcode tool` share it.
- Tools must never panic, never prompt on stdin, never hang: deadlines, capped output, errors that tell the model the fix.
- Prompts (`crates/portcode-agent/prompts`) and `skills/rustport` are for open-weight models: short numbered rules, an example per tool, no time-dependent text before the `date:` line. Tests enforce prompt size and prefix stability.
- Facts about an original go in the ledger as observation / inference / unknown. Never claim parity without a `parity` PASS.
- No secrets, no personal paths in code, tests, screenshots or docs. Screenshots come from `scripts/make_screenshots.sh` / `portcode port shot`, never hand-made.
- Before pushing: `cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace`; if you changed output rendering, rerun `scripts/make_screenshots.sh` and `examples/wordstat/demo.sh`.
- UI: `crates/portcode-cli/src/tui` (App = state/update, view = render, svg = headless screenshot). Real runs and `portcode snapshot` share them; keep `App` free of terminal I/O so it stays testable.
- Installers: `install.sh` must keep verifying sha256; test it with `PORTCODE_DOWNLOAD_URL` against a local `packaging/package.sh` archive.
- Deep mode: tools that look inside the original (`disasm`, `emulate`, `decompile`, `hexdump`) must call `ctx.need_deep(..)`; `bash` refuses inspectors in clean-room. Engines install only under `engines::dir()`, only after `ctx.permit`, and downloads need a published sha256.
- Remote control (`crates/portcode-cli/src/remote`): the key secret never goes over the wire (HMAC challenge); keep the Origin allowlist, the 127.0.0.1 default and the no-slash-commands rule; `App` only fills `outbox`, the event loop publishes it.
