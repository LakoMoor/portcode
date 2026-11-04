# Agent installation and invocation

One standard `SKILL.md` is shared by every supported agent. The installer copies the
whole scripts/assets/references bundle. OpenCode and Pi additionally get a small
command adapter pointing to that installed bundle, not a second workflow.

| Agent | User skill directory | Project skill directory | Explicit invocation |
|---|---|---|---|
| PortCode | built in (embedded) or `portcode skill export <dir>` | `.portcode/skills/rustport` | `portcode -a port` (agent embeds the skill) |
| Codex | `~/.agents/skills/rustport` | `.agents/skills/rustport` | `/skills`, select `rustport`; or `$rustport request` |
| Claude Code | `~/.claude/skills/rustport` | `.claude/skills/rustport` | `/rustport request` |
| OpenCode | `~/.config/opencode/skills/rustport` | `.opencode/skills/rustport` | `/rustport request` through command adapter |
| Pi | `~/.pi/agent/skills/rustport` | `.pi/skills/rustport` | `/rustport request` through prompt adapter; native `/skill:rustport request` |
| DeepSeek Harness (DSH) | `~/.dsh/skills/rustport` | `.dsh/skills/rustport` | `/rustport request` with filesystem provider and user invocation enabled |
| Hermes | `~/.hermes/skills/rustport` | `.hermes/skills/rustport` | `/rustport request` |

OpenCode adapters live in `commands/rustport.md`; Pi adapters in `prompts/rustport.md`
under their respective config roots. Both pass `$ARGUMENTS` as the user request.
Codex's supported slash entrypoint is `/skills`, not a fabricated `/rustport` command.
It also supports `$rustport` directly. No deprecated custom-prompt shim is installed.

The installer honors `CLAUDE_CONFIG_DIR`, `XDG_CONFIG_HOME` (OpenCode),
`PI_CODING_AGENT_DIR`, `DSH_HOME`, and `HERMES_HOME` for user installs.
`--home PATH` explicitly selects a different home and ignores those overrides.
Project roots must be inside the repository recognized by the selected agent;
DSH and Hermes use the nearest `.git` root. Hermes requires project trust, and Pi
may require trusting project prompt templates. The installer does not change trust,
tool permissions, MCP configuration, model settings, or launch an agent.

After installation use `/reload` in Pi; reload/restart other clients if needed.
DSH watches skill roots. Hermes can refresh skill commands with `/reload-skills`.
A host may disable user commands or automatic selection; installing files does not
override those settings.

## Runtime portability

Use the agent's native file, shell, browser, and MCP tools by capability; never assume
Claude-only tool names or environment variables. Resolve `$RP` from the loaded
SKILL.md path. If a native skill loader is unavailable, read that file explicitly
and follow it. On Windows use Python to install; shell helpers require Bash (for
example Git Bash or WSL), plus Python 3 and Cargo. The wgpu template has its own
platform/GPU requirements. A helper being installed is not proof that the runtime
or original game can be executed in the current environment.

## Upstream contracts (checked 2026-10-29)

- Codex discovery: https://developers.openai.com/codex/skills/
- Codex selector: https://developers.openai.com/codex/cli/slash-commands/
- Claude: https://code.claude.com/docs/en/skills
- OpenCode skills: https://opencode.ai/docs/skills/
- OpenCode commands: https://opencode.ai/docs/commands/
- Pi skills: https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md
- Pi prompts: https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/prompt-templates.md
- DSH provider: https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/skill/skill-filesystem/README.md
- Hermes: https://hermes-agent.nousresearch.com/docs/user-guide/features/skills

Installer tests exercise generated layouts, command arguments, and complete bundle
copies in temporary directories. They do not constitute end-to-end testing of all
six interactive agent clients or guarantee the quality of the resulting game port.
