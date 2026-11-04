# Clean-room vs deep mode, authorisation, consent

## Authorisation first
Work only on software the user owns, has permission to study, or may examine for interoperability/preservation under the applicable law and licence. If that is unclear, ask once and stop. Never publish original assets, extracted proprietary data, saves, or decompiler output; keep them in `.portcode/` (git-ignored). A port may contain recovered *rules and constants*; it must not contain transliterated pseudo-code or copied assets.

## Two modes (record the choice in `docs/PORT.md`)
| Mode | Allowed | Evidence it yields |
|---|---|---|
| **clean-room** (default) | Observe from outside, as a user or visitor would: run it, feed inputs, read outputs, screenshots, network behaviour, file formats of *your own* data, public docs | mostly `observation` |
| **deep** | Also look inside: disassemble, decompile, trace calls, read source | mostly `inference`, flagged with the engine/tool used |

Deep mode never replaces observation: recovered rules still need a golden captured from the running original.

## Executing the original
- Run it only when the user approved execution; isolated (VM/container for unknown binaries), bounded (time, output size), stdin closed, no real user data, no network unless the scenario needs it.
- A missing oracle is not a pass: record an `unknown` and lower the fidelity claim.

## Effects that need consent
Running foreign code, mutating a device, installing tools or MCP servers, writing outside the project, network fetches. Ask before; log in the decision log after.
