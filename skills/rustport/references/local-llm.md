# Working well with open-weight models (DeepSeek, Qwen, Llama, Mistral, GLM, Kimi …) and self-hosted endpoints

These models follow a procedure very well and drift on long, vague instructions. The skill is written to exploit that. If you are running on one, follow this page; if you maintain the skill, keep to it.

## How the skill is shaped for them
- **Short head, long tail.** `SKILL.md` is a router (~100 lines). Detail lives in `references/` and is loaded only for the phase that needs it (the `skill` tool takes `file`).
- **Phases with "Done when".** A checklist the model can verify beats prose it has to interpret.
- **Rules at both ends.** Hard rules at the top, failure handling and style at the bottom (open models weight the start and end of context most).
- **Examples in every tool.** Each tool description carries an `Example arguments:` line; copy its shape exactly.
- **Stable prefix.** The system prompt does not change between turns (no timestamps), so llama.cpp / vLLM / SGLang prefix caches hit and turns stay fast.

## What to do as the model
1. One tool call per step unless the calls are independent reads. Never narrate a tool call in prose — emit it.
2. Arguments are strict JSON. Numbers are numbers. Paths are relative to the project.
3. Keep outputs small: `read` with `offset/limit`, `grep` first, `bash` with `| head -50`.
4. Think briefly, then act. Do not restate the task. Reasoning goes in the reasoning channel; the visible answer is short.
5. After a failed command, read the first error only, fix it, rerun. Do not rewrite the file.
6. When the context fills, PortCode compacts older turns. Keep durable facts in `docs/PORT.md` and the ledger, not only in chat.
7. End each turn: **done / next step / blocked-on**.

## Small / "flash" models (≤ ~14B or latency-tuned)
Slices ≤ 100 LOC. Delegate searching to the `explore` sub-agent. Ask the user for the decision instead of guessing between two plausible designs. Prefer `edit` over `write`. Re-state the current phase from `docs/PORT.md` at the start of a session.

## Tool-calling quirks PortCode absorbs (so you do not have to)
Tool calls written as text (`<tool_call>{…}</tool_call>`, fenced JSON), truncated or trailing-comma JSON arguments, quoted numbers, inline `<think>` blocks, `reasoning_content`, servers that reject optional sampling fields. In other agents, avoid relying on these working.

## Measure, do not assume
Profiles in `portcode.json` (`models`) are starting points. Check your model: run `portcode eval` (tool-call round-trip, JSON validity, edit accuracy) and keep the numbers next to the profile.
