# Explicit original-vs-Rust scenario files

Example `scenario.json`:

```json
{
  "schema": 1,
  "original_argv": ["/absolute/path/to/original-probe", "--headless", "--seed", "123"],
  "port_argv": ["/absolute/path/to/rust-probe", "--headless", "--seed", "123"],
  "stdin_file": "input.txt",
  "timeout_seconds": 15,
  "env": {"RUST_LOG": "off"}
}
```

Each executable **must emit JSONL on stdout**, one nonempty JSON object per tick or event, in the same schema, and exit 0 for a passing comparison. Store the scenario and minimal non-proprietary inputs under tests. Do **not** put original executable bytes or game assets in Git.

```bash
python3 scripts/oracle_runner.py scenario.json --output ./evidence/run-01 \
  --allow-execute-original --compare
```

This flag acknowledges that you chose to execute the original; it is **not a sandbox**. The runner clears most environment variables and separates basic home/config/cache directories. It does not prevent network requests, writes outside the scratch home, or malicious behavior: run untrusted inputs in a real isolated VM with access restricted to copies of game content. Files are not copied or downloaded automatically by this tool.

Output: `original.jsonl`, `port.jsonl`, stderr logs, SHA-256 capture identity and `parity.json` (if comparing). Preserve the reference pin alongside documentation. An executable hash proves identity, not correctness of the input scenarios or independence of instrumentation.
