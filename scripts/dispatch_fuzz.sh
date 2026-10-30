#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
fi

log_dir="${HOME}/.tmp/adotenv-fuzz"
mkdir -p "$log_dir"
log_file="${log_dir}/$(date -u '+%Y%m%dT%H%M%SZ').log"
exec > >(tee -a "$log_file") 2>&1

if [[ -z "${GH_TOKEN:-}" ]]; then
    GH_TOKEN="$(gh auth token --user asamarts)"
    export GH_TOKEN
fi

if $dry_run; then
    echo "dispatch_fuzz: dry run"
    echo "repo=$repo_root"
    echo "event=workflow_dispatch"
    gh auth status
    exit 0
fi

before="$(gh run list --workflow fuzz.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
gh workflow run fuzz.yml --ref main

run_id=""
for _ in $(seq 1 24); do
    sleep 5
    candidate="$(gh run list --workflow fuzz.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
    if [[ -n "$candidate" && "$candidate" != "$before" ]]; then
        run_id="$candidate"
        break
    fi
done

if [[ -z "$run_id" ]]; then
    echo "dispatch_fuzz: no new workflow run appeared" >&2
    exit 1
fi

echo "dispatch_fuzz: run_id=$run_id"
gh run watch "$run_id" --exit-status --interval 20
