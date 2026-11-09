# Roadmap

- [ ] `apply_patch` tool; `question` tool for structured user decisions
- [ ] Native Anthropic/Google protocols in `portcode-llm`
- [ ] TUI: diff viewer, model/session pickers, themes, mouse text selection, redraw history on `/resume`
- [ ] MCP: HTTP transport, resources, prompts; `portcode mcp add rea` wizard
- [ ] `portcode serve` (HTTP/SSE) + SDK
- [ ] `portcode eval` suite with stored per-model results and recommended profiles (DeepSeek, Qwen releases)
- [ ] Native binary analysis in Rust (format parsers → disassembly) so REA/Ghidra are optional
- [ ] Differential-fuzz harness as a tool (`fuzz`), golden recorder for GUI/wgpu apps (offscreen PNG)
- [ ] `portcode port site --publish` GitHub Pages workflow generator for ports
- [ ] Windows binaries + `install.ps1` (needs a portable `bash` tool; today use WSL)
- [ ] Windows support for `bash` tool (process groups, `cmd`/PowerShell)

- [x] Deep mode with Capstone, Unicorn and Kuna tools; engines installed next to the binary
- [ ] ImHex pattern evaluation without the GUI; angr/Frida wrappers as first-class tools; Windows engines
- [x] Remote control: `portcode serve`, `/remote`, browser page on GitHub Pages, HMAC-authenticated keys with view/full scope
- [ ] Optional end-to-end-encrypted relay for machines that cannot run a tunnel; push notifications for permission asks
