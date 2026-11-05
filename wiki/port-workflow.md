# The port workflow

PortCode's reason to exist: port a program and *prove* the port behaves the same. The `port` agent follows the rustport method and the runtime enforces its finish line.

## The idea

1. **Pin the original.** Its exact build is hashed (sha256), so every later claim is about a known artifact.
2. **Write down facts, with their weight.** Each fact goes into the ledger as one of three kinds:

   | Kind | Meaning |
   |---|---|
   | `observation` | I saw or ran it. |
   | `inference` | I worked it out from observations. |
   | `unknown` | I do not know yet. |

   An unknown stays open until something resolves it. PortCode never claims parity without a passing `parity` run.
3. **Port one module at a time.** Each module is compared with golden output captured from the **original**, not from your port.
4. **Check with parity.** A differential run compares the port's output with the golden files, with tolerances for floating point.
5. **Show it.** A screenshot per feature and a generated site for GitHub Pages.

## Start

Put the original in `original/` and run:

```sh
portcode -a port
```

or type `/create` and answer the questions. The agent begins by reading the ledger and `docs/PORT.md` if they exist, and says which phase it is in.

## The finish gate

`port` does not stop early. If any of these is true when it tries to finish, it is sent back to work and given more steps:

- a behaviour of the original (an option, an output format, an exit code, an error message) is not in the todo list,
- a todo is still open,
- the last `parity` run did not pass on the final code.

The only exit is `blocked`: the agent stops and shows a **Needs your decision** card with its questions. Your answer resumes the run.

## The tools

| Tool | Does |
|---|---|
| `survey` | inventories a source tree: languages and size, build files, engine and graphics hints, assets, tests, risk hotspots |
| `pin` | pins the original build (sha256, size, format, version) in `.portcode/reference.lock.json` and verifies it later |
| `evidence_add` | adds an observation, inference or unknown to the ledger |
| `evidence_list` | reads the ledger, by subject or only the unknowns |
| `parity` | compares golden output recorded from the original with the port's (numbers within tolerance, PNG per channel); missing, extra or empty output is a failure |
| `screenshot` | runs a command, saves its real output as an SVG terminal screenshot in `docs/screenshots/` and records it as an observation |
| `scaffold` | creates a Rust workspace: `<name>-core` (headless) plus `<name>` (binary), `docs/PORT.md`, `tests/golden`, `docs/screenshots` |
| `site_build` | generates the GitHub Pages site in `docs/site/` from `docs/PORT.md`, the ledger and the screenshots |

The same things exist on the command line as `portcode port survey|pin|parity|scaffold|shot|site|render|ledger`.

## Where the port is created

Always inside the folder you opened PortCode in. `scaffold` creates `<name>-rs/` there (or the `dest` you give, relative to that folder), and the agent is told to use relative paths and never write beside or above the project. Anything outside the folder needs your approval first.

## Where things live

| Path | Holds |
|---|---|
| `original/` | the program being ported |
| `.portcode/evidence/` | the evidence ledger |
| `.portcode/reference.lock.json` | the pinned build of the original |
| `.portcode/sessions/` | saved sessions |
| `docs/PORT.md` | the port plan and status |
| `docs/SPEC.md` | the written spec, when you use the two-session setup |

## An example

`examples/wordstat` in the repository is a complete small port with real data, and `docs/ports/wordstat/` is the site generated from it. Run `examples/wordstat/demo.sh` to replay it.

## Two sessions for a strict clean room

For the strictest separation, run a `reverse --deep` session that studies the original and writes `docs/SPEC.md` in its own words. Then run a `port --clean-room` session that builds the port from that spec alone and never looks inside the original. See [Clean-room and deep mode](modes.html).
