# Contributing

PortCode is a Rust workspace. This page is the short version; `AGENTS.md` in the repository is the full guide for people and agents.

## Layers

```text
evidence (L0) → port, llm (L1) → agent (L2) → cli (L3)
```

Lower layers never import higher ones. See [Architecture](architecture.html).

## Rules that matter

- `unsafe` is forbidden. Outside tests, no `unwrap`, `expect` or indexing on values that come from input (lints enforce it in L0 to L2).
- Everything user-visible is a `Tool` in `crates/portcode-agent/src/tools`. The app, the agent and `portcode tool` share it.
- Tools never panic, never prompt on stdin, never hang: deadlines, capped output, errors that tell the model the fix.
- Prompts in `crates/portcode-agent/prompts` and the `rustport` skill are written for open-weight models: short numbered rules, an example per tool, nothing time-dependent before the `date:` line. Tests enforce size and prefix stability.
- Facts about an original go in the ledger as observation, inference or unknown. Never claim parity without a `parity` pass.
- No secrets and no personal paths in code, tests, screenshots or docs. Screenshots come from `scripts/make_screenshots.sh` or `portcode port shot`, never by hand.
- Deep tools must call `ctx.need_deep(..)`. Engines install only under the engines folder, only after approval, and downloads need a published sha256.
- The app's state (`App`) holds no terminal I/O, so it stays testable. The event loop runs the tools.

## Before you push

```sh
cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
```

If you changed how output is drawn, rerun `scripts/make_screenshots.sh` and `examples/wordstat/demo.sh`. If you changed the docs, rerun `scripts/build_site.sh`.

## Where the docs come from

The wiki pages are Markdown in `wiki/` (and a few in `docs/`). `scripts/build_wiki.py` turns them into `docs/wiki/`. The slash-command table, the tool table and the command-line page are generated from the code, so they stay correct. Do not edit the generated HTML.

## Releases

Pushing a `v*` tag runs the release workflow, which builds the four archives with checksums and publishes them. `install.sh` must keep verifying sha256; test it with `PORTCODE_DOWNLOAD_URL` against a local `packaging/package.sh` archive.
