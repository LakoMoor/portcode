# Pitfalls — mistakes that cost time, and the rule that prevents each

- **Porting from reading, not from running.** Code review said "obvious", the original did something else. → Run the original; record an observation.
- **Tolerance widened to make a test pass.** Hid a real bug for weeks. → Widen only with a written cause in `docs/PORT.md`.
- **Golden not regenerable.** Nobody knew how it was captured. → Keep the capture command in the evidence record and the patch in `tests/original_patch/`.
- **Static analysis claimed as behaviour.** → `inference` until a run confirms it.
- **Version drift.** Findings applied to a different build. → `pin` first; re-pin and re-run on update.
- **Everything ported before anything ran.** → Connect a real-content vertical slice early.
- **wgpu by reflex.** A CLI got a GPU dependency. → Decision table in `assets/mappings.md`.
- **Mass rewrite after a failure.** → 3-strike rule: stop, document, change approach.
- **`HashMap` order leaked into output.** → `BTreeMap`/`IndexMap` where order is observable.
- **Edit tool "not found".** Whitespace or stale read. → `read` again, copy the exact text, include 2–3 context lines.
- **Command hung the agent.** A tool waited on stdin. → Non-interactive flags, closed stdin, deadlines (PortCode's `bash` does this).
- **Screenshot of something that was not run.** → Screenshots are produced by the tool from real output, never drawn.
- **Secrets committed.** → Keys in env or `portcode auth`; `.portcode/` is git-ignored.
