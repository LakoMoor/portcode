# Team mode

Team mode lets one agent lead a small team that works at the same time: a project manager, a tech lead, researchers, developers, a tester and a reviewer.

## Turning it on

Either way adds the `team` agent to the `tab` cycle:

```json
{ "team": { "enabled": true, "parallel": 3 } }
```

or, for the running session, `/team on` (and `/team off`). `parallel` is how many workers may run at once: 3 by default, 8 at most.

## How a run goes

You switch to the `team` agent and describe the goal. The orchestrator does not write code itself. It follows this order:

1. **Plan.** It asks `pm` for scope, steps and done-criteria, and for code work `techlead` for a design and which files each developer owns.
2. **Do.** It runs researchers on different questions and developers on **different files** together in one `team` call.
3. **Check.** `tester` runs or writes the tests, `reviewer` reads the changes. Failures go back to a developer with the exact error.
4. **Verify.** The orchestrator runs the build or tests itself before it says it is done. It never takes a worker's word for it.
5. **Report.** What each role did, the verified result, and what is left.

While workers run, the transcript shows `team ▸ developer: api started` and `team ■ developer: api done` lines.

## The roles

| Role | Reads | Edits | Job |
|---|---|---|---|
| `pm` | yes | no | scope, numbered steps, done-criteria, risks |
| `techlead` | yes | no | design, files per developer, interfaces, the check to run |
| `researcher` | yes | no | answers one question with file:line evidence |
| `developer` | yes | yes | implements the files it was assigned |
| `tester` | yes | tests only | runs the project's tests and reports each failure |
| `reviewer` | yes | no | lists problems with severity, file:line and a fix |

Workers get fresh context and only the tool list their role needs.

## The `team` tool

The orchestrator calls one tool with up to 8 tasks:

```json
{"tasks": [
  {"role": "researcher", "task": "Where is the config file parsed? Answer with file:line."},
  {"role": "developer", "name": "api", "task": "Add a --json flag in src/cli.rs ..."}
]}
```

A failing worker is reported as `FAILED` with its error and does not stop the others.

## Permissions and cost

Workers share your permission dialog and rules, so requests queue up; `/yolo` approves them all. Each worker is its own model conversation: a team run uses several times the tokens of a single agent. On a local model with a small context window, keep `parallel` low.

> [!TIP]
> Team mode pays off on work that splits cleanly into separate files or questions. For a small change, `build` is faster and cheaper.
