# Site and screenshots — every port gets proof you can look at

## What "done" looks like
For each ported feature in `docs/PORT.md` there is at least one **evidence record with an attached screenshot** showing the original and the port doing the same thing, and the generated site shows it next to the claim.

## Capture
| Target | How | Result |
|---|---|---|
| CLI / TUI / library demo | `screenshot` tool (or `portcode port shot`): runs the command, saves a terminal-style SVG with the real captured output, records an observation with the output's sha256 | `docs/screenshots/<name>.svg` |
| GUI / game / web | OS capture or an MCP tool (REA `capture_*_scenario`); save PNG to `docs/screenshots/`; attach with `evidence_add` (`attachments: ["screenshots/<file>"]`) | PNG + record |
| Visual parity | `parity` on golden vs port PNGs (per-channel diff, size must match) | PASS/FAIL |

Name pairs `<feature>-original` / `<feature>-port`. Never stage or edit a screenshot; if it shows a failure, record the failure.

## Build and publish
1. `site_build` (or `portcode port site`) → `docs/site/` (index.html, style.css, screenshots/, `.nojekyll`).
2. Open `docs/site/index.html` and look at it: plan, evidence grouped by feature, open unknowns, gallery.
3. GitHub Pages: Settings → Pages → deploy from branch, folder `/docs/site` (or use the provided workflow).
4. Do not publish original assets or private data; screenshots of proprietary content need the same authorisation as the work itself.
