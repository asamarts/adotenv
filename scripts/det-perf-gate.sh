#!/usr/bin/env bash
# Deterministic instruction/cycle gate for adotenv hot paths.
#
# Builds the base and head revisions in the same session, runs the gungraun
# Callgrind benches under the same toolchain and Valgrind, and lets the
# per-bench `soft_limits` fail the job. The comparison is instruction-based, so
# co-tenant load on a hosted runner cannot fail it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

head="$(git rev-parse HEAD)"
base="${BASE_SHA:-}"
if [[ -z "$base" ]]; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    git fetch --no-tags origin "$GITHUB_BASE_REF" >/dev/null 2>&1 || true
    base="$(git merge-base "origin/$GITHUB_BASE_REF" HEAD)"
  else
    base="$(git merge-base origin/main HEAD)"
  fi
fi
if [[ -z "$base" || "$base" == "$head" ]]; then
  echo "det-perf: no distinct base to compare; skipping"
  exit 0
fi
if ! git cat-file -e "$base:adotenv-bench/benches/det_parse.rs" 2>/dev/null; then
  echo "det-perf: base $base predates det_parse; bootstrap skip"
  exit 0
fi

if ! command -v valgrind >/dev/null 2>&1; then
  sudo apt-get update -qq
  sudo apt-get install -y -qq valgrind
fi

version="$(awk '/^name = "gungraun"$/{f=1; next} f && /^version = /{gsub(/"/,"",$3); print $3; exit}' Cargo.lock)"
if [[ -z "$version" ]]; then
  echo "det-perf: could not read gungraun version from Cargo.lock" >&2
  exit 1
fi
have="$(~/.cargo/bin/gungraun-runner --version 2>/dev/null | awk '{print $NF}' || true)"
if [[ "$have" != "$version" ]]; then
  cargo install gungraun-runner --version "$version" --locked
fi
export PATH="$HOME/.cargo/bin:$PATH"

worktree="$(mktemp -d)"
cleanup() {
  git worktree remove --force "$worktree" >/dev/null 2>&1 || true
  rm -rf "$worktree"
}
trap cleanup EXIT

export CARGO_TARGET_DIR="$PWD/target/det-gate"
echo "det-perf: baseline $base"
git worktree add --detach "$worktree" "$base" >/dev/null
(cd "$worktree" && cargo bench -p adotenv-bench --bench det_parse --locked -- --save-baseline=det_base)

echo "det-perf: head $head"
cargo bench -p adotenv-bench --bench det_parse --locked -- --baseline=det_base
echo "det-perf: PASS"
