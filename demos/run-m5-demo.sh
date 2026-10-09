#!/usr/bin/env bash
# Build the current desktop and open an editable copy of the integrated demo.
set -euo pipefail
repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "$repo_dir"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
run_dir="$(mktemp -d "${TMPDIR:-/tmp}/fold-m5-demo.XXXXXXXX")"
cp -- demos/m5b-integrated.fold "$run_dir/Make It Move.fold"
printf 'Opening your editable demo: %s\n' "$run_dir/Make It Move.fold"
printf 'Use Save As in Fold to keep it in your project folder.\n'
exec cargo run -p fold-app --features desktop --bin fold-desktop --locked -- --project "$run_dir/Make It Move.fold"
