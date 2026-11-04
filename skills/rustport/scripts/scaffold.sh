#!/usr/bin/env bash
# rustport scaffold — create a Cargo workspace: <name>-core (pure logic, no window/GPU) + <name> (app/wgpu frontend).
# Usage: scaffold.sh <name> <dest_dir> [--no-gpu]
set -euo pipefail
name="${1:?name}"; dest="${2:?dest}"; gpu=1; [ "${3:-}" = "--no-gpu" ] && gpu=0
[[ "$name" =~ ^[a-zA-Z][a-zA-Z0-9_-]*$ ]] || { echo "Invalid Rust package name: $name" >&2; exit 2; }
skill="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$dest"; cd "$dest"
[ -e Cargo.toml ] && { echo "Cargo.toml already exists in $dest" >&2; exit 1; }

cat > Cargo.toml <<TOML
[workspace]
resolver = "2"
members = ["crates/${name}-core", "crates/${name}"]

[profile.dev]
opt-level = 1
[profile.release]
lto = "thin"
codegen-units = 1
TOML

mkdir -p "crates/${name}-core/src" "crates/${name}/src" tests/golden docs
cat > "crates/${name}-core/Cargo.toml" <<TOML
[package]
name = "${name}-core"
version = "0.1.0"
edition = "2021"
[dependencies]
anyhow = "1"
TOML
cat > "crates/${name}-core/src/lib.rs" <<'RS'
//! Deterministic, window-free core. No GPU, no OS handles, no wall-clock time, no global RNG.
//! All time enters as `dt`, all randomness as an explicit seeded generator, so golden tests are reproducible.
RS

if [ "$gpu" = 1 ]; then
  cp "$skill/assets/wgpu_template/Cargo.toml" "crates/${name}/Cargo.toml"
  sed -i.bak "s/__NAME__/${name}/" "crates/${name}/Cargo.toml" && rm "crates/${name}/Cargo.toml.bak"
  # strip per-crate profiles (workspace root owns them)
  python3 - "crates/${name}/Cargo.toml" <<'PY'
import sys,re
p=sys.argv[1]; s=open(p).read()
s=re.split(r'\n\[profile', s)[0].rstrip()+"\n"
open(p,'w').write(s)
PY
  printf '%s-core = { path = "../%s-core" }\n' "$name" "$name" >> "crates/${name}/Cargo.toml"
  cp "$skill/assets/wgpu_template/src/main.rs" "$skill/assets/wgpu_template/src/shader.wgsl" "crates/${name}/src/"
else
  cat > "crates/${name}/Cargo.toml" <<TOML
[package]
name = "${name}"
version = "0.1.0"
edition = "2021"
[dependencies]
${name}-core = { path = "../${name}-core" }
anyhow = "1"
TOML
  printf 'fn main() -> anyhow::Result<()> {\n    Ok(())\n}\n' > "crates/${name}/src/main.rs"
fi

cp "$skill/assets/PORT_TEMPLATE.md" docs/PORT.md
printf 'target/\n' > .gitignore
cargo check 2>&1 | tail -3
echo "scaffolded: $dest"
