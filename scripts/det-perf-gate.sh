#!/usr/bin/env bash
# Deterministic instruction/cycle gate for adotenv hot paths.
#
# Builds the base and head revisions in the same session, runs the gungraun
# Callgrind benches under the same toolchain and Valgrind, and lets the
# per-bench `soft_limits` fail the job. The comparison is instruction-based, so
# co-tenant load on a hosted runner cannot fail it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

tooling_failure() {
  echo "::error title=Deterministic perf gate::$1 (tooling failure, not a perf signal); the gate did not run" >&2
  exit 1
}

head="$(git rev-parse HEAD)"
worktree=""
base_target="$PWD/target/det-gate-base"
head_target="$PWD/target/det-gate-head"
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
  installed=false
  for attempt in 1 2 3; do
    if sudo apt-get update -qq && sudo apt-get install -y -qq valgrind; then
      installed=true
      break
    fi
    echo "det-perf: valgrind install failed (attempt ${attempt}); retrying" >&2
    sleep 10
  done
  if [[ "$installed" != "true" ]] || ! command -v valgrind >/dev/null 2>&1; then
    tooling_failure "could not install valgrind"
  fi
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
  if [[ -n "$worktree" ]]; then
    git worktree remove --force "$worktree" >/dev/null 2>&1 || true
  fi
  rm -rf "$worktree" "$base_target" "$head_target"
}
trap cleanup EXIT

rm -rf "$base_target" "$head_target"
echo "det-perf: baseline $base"
git worktree add --detach "$worktree" "$base" >/dev/null
(cd "$worktree" && CARGO_TARGET_DIR="$base_target" cargo bench -p adotenv-bench --bench det_parse --locked -- --save-baseline=det_base)
mkdir -p "$head_target"
cp -a "$base_target/gungraun" "$head_target/"

echo "det-perf: head $head"
CARGO_TARGET_DIR="$head_target" cargo bench -p adotenv-bench --bench det_parse --locked -- --baseline=det_base
echo "det-perf: PASS"
