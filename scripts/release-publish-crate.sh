#!/usr/bin/env bash
# Package, publish and verify one workspace crate through crates.io.
#
# Used by the independent linter and LSP dispatch jobs. The tag release job has
# the same checks inline because it publishes several crates in dependency
# order. The caller supplies the registry token through
# CARGO_REGISTRY_TOKEN and is responsible for attesting and uploading the
# files in release-assets/.
set -euo pipefail

package="${1:?usage: release-publish-crate.sh PACKAGE}"
token="${CARGO_REGISTRY_TOKEN:-}"
if [[ -z "$token" ]]; then
  echo "::error::CARGO_REGISTRY_TOKEN is not set for $package" >&2
  exit 1
fi

metadata="$(cargo metadata --no-deps --format-version 1)"
version="$(
  python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == sys.argv[1]))' \
    "$package" <<<"$metadata"
)"

index_path() {
  local package="$1"
  case "${#package}" in
    1) echo "1/$package" ;;
    2) echo "2/$package" ;;
    3) echo "3/${package:0:1}/$package" ;;
    *) echo "${package:0:2}/${package:2:2}/$package" ;;
  esac
}

index_body() {
  local package="$1"
  curl -fsSL --max-time 10 -H 'User-Agent: adotenv-release' \
    "https://index.crates.io/$(index_path "$package")"
}

index_checksum() {
  local package="$1"
  local version="$2"
  index_body "$package" | python3 -c '
import json
import sys
version = sys.argv[1]
for line in sys.stdin:
    try:
        row = json.loads(line)
    except json.JSONDecodeError:
        continue
    if row.get("vers") == version:
        print(row.get("cksum", ""))
        break
' "$version"
}

wait_for_index() {
  local package="$1"
  local version="$2"
  local body=""
  for _ in $(seq 1 90); do
    if body="$(index_body "$package")" \
      && grep -Fq "\"vers\":\"$version\"" <<<"$body"; then
      echo "::notice::$package $version is visible in the sparse index"
      return 0
    fi
    sleep 2
  done
  echo "::error::$package $version did not appear in the sparse index within 180 seconds" >&2
  return 1
}

rm -rf release-assets
mkdir -p release-assets

echo "::group::Packaging $package $version"
cargo package -p "$package" --locked
crate="target/package/${package}-${version}.crate"
if [[ ! -f "$crate" ]]; then
  echo "::error::cargo package did not produce $crate" >&2
  exit 1
fi
asset="release-assets/${package}-${version}.crate"
cp "$crate" "$asset"
(cd release-assets && sha256sum "${package}-${version}.crate" > "${package}-${version}.crate.sha256")
local_checksum="$(sha256sum "$crate" | awk '{print $1}')"
echo "::endgroup::"

echo "::group::Publishing $package $version"
CARGO_REGISTRY_TOKEN="$token" cargo publish -p "$package" --locked
wait_for_index "$package" "$version"
remote_checksum="$(index_checksum "$package" "$version")"
if [[ -z "$remote_checksum" ]]; then
  echo "::error::crates.io index did not report a checksum for $package $version" >&2
  exit 1
fi
if [[ "$remote_checksum" != "$local_checksum" ]]; then
  echo "::error::$package $version checksum mismatch: local $local_checksum, crates.io $remote_checksum" >&2
  exit 1
fi
echo "::notice::$package $version checksum matches crates.io ($local_checksum)"
echo "::endgroup::"
