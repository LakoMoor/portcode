# Static-inferred VPD name validator

This is a bounded reconstruction from binary assembly, **not verified parity**.
It validates UTF-8 names for VPD syntax hazards and whitespace. It does not parse
or export VPD files. The CLI is a new convenience interface, not an openMMD CLI.

```
cargo test --manifest-path port/Cargo.toml
cargo run --manifest-path port/Cargo.toml -- 'センター'
cargo run --manifest-path port/Cargo.toml -- '' --allow-empty
```

No dependencies, GPU, unsafe code, or file access. See ../PORT.md and
../COMPATIBILITY.md for original-side evidence and limitations. The original's
MIT notice is retained at ../evidence/ORIGINAL-LICENSE.
