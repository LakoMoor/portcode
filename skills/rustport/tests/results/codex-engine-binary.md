# Codex engine and binary-only follow-up

Date: 2026-10-29. Client: Codex CLI 0.165.0-alpha.1.2. These are bounded
integration investigations, not successful whole-engine reconstruction claims.

## L2D source engine

Reference: LakoMoor/L2D, revision 0ed90671703c40edd063111a140440ac64a9aea0.
Codex attempted the original CMake build and found missing submodules and a
modern-CMake compatibility failure. Its sandbox dependency fetch failed DNS.
It identified uninitialized and out-of-bounds color reads in the original.

It implemented a small Rust lifecycle/event loop with a local SDL bridge.
Quit and Escape traces and a repaired black framebuffer matched a separate
instrumented C++ harness. That harness substituted ImGui/logging and repaired
undefined behavior: these are harness-derived checks, not unmodified-original
UI parity. The bounded visible Rust run failed due to display availability.
The result supports only a narrow lifecycle slice. No ImGui or game parity.

## Released Windows EXE, no application source

Reference: LakoMoor/openMMD v0.1.0-preview.2, official Windows release archive.
Archive SHA-256 matched the published checksum:
`037bb7afa684e67accc08165c60e4be20f11f9047a5715438d4a8c281e729627`.
EXE SHA-256:
`be9c3129f926fdc95019e67b7beeb61860d08ef61d8b648509cb849465dae366`.

The parent downloaded the release; the test Codex itself loaded rustport and
reverse-engineer-anything, called REA open_binary and binary_session, and selected
the binary-only route. No application source was supplied or consulted.
REA recognized a 15,649,280-byte native AMD64 PE32+ executable. Connected MCP
reported Ghidra 12.1.4 available even though shell doctor lacked its environment.
A string-search analysis request returned no result during the approximately
three-minute bound; the caller was stopped and close_binary acknowledged cleanup.
This does not establish a missing or failed Ghidra engine, nor completed analysis.

No Wine runtime was found in the checked locations. An executable temporary copy
failed direct macOS launch with Exec format error: no application behavior ran.
Two identity/header checks passed; zero application goldens or parity checks ran.
No Rust behavior slice or substitute GUI was fabricated. The result verifies
skill routing and honest blocker handling, not binary reconstruction capability.

The test exposed guidance improvements in assets/rea.md: distinguish the MCP
provider environment from shell diagnostics, separate target opening from
completed analysis/execution, and label stand-in harness evidence explicitly.
No host configuration or new runtime was installed.
