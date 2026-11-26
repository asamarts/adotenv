#!/usr/bin/env bash
# Create or update a permanent GitHub Release for an independent companion or
# adapter crate dispatch (`<crate>-v<version>`), marked non-latest so lockstep
# core `vX.Y.Z` releases remain the repository's latest release.
set -euo pipefail

crate="${1:?usage: release-companion-github.sh CRATE VERSION ASSETS_DIR [TARGET_SHA]}"
version="${2:?usage: release-companion-github.sh CRATE VERSION ASSETS_DIR [TARGET_SHA]}"
assets_dir="${3:?usage: release-companion-github.sh CRATE VERSION ASSETS_DIR [TARGET_SHA]}"
target_sha="${4:-${GITHUB_SHA:-}}"

if [[ -z "$target_sha" ]]; then
  echo "::error::target commit SHA is required (pass arg 4 or set GITHUB_SHA)" >&2
  exit 1
fi

crate_file="${assets_dir}/${crate}-${version}.crate"
sha_file="${assets_dir}/${crate}-${version}.crate.sha256"
for path in "$crate_file" "$sha_file"; do
  if [[ ! -f "$path" ]]; then
    echo "::error::missing release asset: $path" >&2
    exit 1
  fi
done

tag="${crate}-v${version}"
notes="$(mktemp)"
trap 'rm -f "$notes"' EXIT

if ! gh release view "$tag" >/dev/null 2>&1; then
  python3 scripts/release-extract-changelog.py "${crate} ${version}" > "$notes"
  gh release create "$tag" \
    --target "$target_sha" \
    --latest=false \
    --title "${crate} ${version}" \
    --notes-file "$notes"
else
  echo "::notice::release $tag already exists"
fi

gh release upload "$tag" "$crate_file" "$sha_file" --clobber
