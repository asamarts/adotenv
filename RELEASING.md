# Releasing

The release workflow publishes five crates in dependency order:

1. `adotenv`
2. `adotenv-process`
3. `adotenv-config`
4. `adotenv-figment`
5. `adotenv-cli`

All five crates are registered with Trusted Publishing. Existing crates publish
through GitHub Actions OIDC. A temporary `CARGO_REGISTRY_TOKEN` is only needed
for a new crate that has no trusted publisher yet.

## Preflight

1. Run the full workspace gate.
2. Confirm all five manifests use the same version and path dependency range.
3. Run the API check against the previous published core version:

   ```console
   cargo semver-checks check-release --baseline-version <previous-version> --release-type patch
   ```

   Review any failure before release. The patch check is intentionally strict
   for pre-1.0 releases.

4. Update package manifests and path dependency constraints explicitly. Do not
   replace every matching version string in `Cargo.lock`. Third-party crates can
   share the same version number.

## Publish

Create and push an annotated tag from the release commit:

```console
git tag -a vX.Y.Z -m "adotenv X.Y.Z"
git push origin vX.Y.Z
```

The workflow:

- runs the quality gate;
- checks that the tag matches every package version;
- skips versions already present on crates.io;
- uses OIDC for existing crates;
- waits for each version in the sparse index before publishing dependents.

## Verify

Check all five crate pages:

- <https://crates.io/crates/adotenv>
- <https://crates.io/crates/adotenv-process>
- <https://crates.io/crates/adotenv-config>
- <https://crates.io/crates/adotenv-figment>
- <https://crates.io/crates/adotenv-cli>

Check the docs roots on docs.rs and confirm the GitHub Release points at the
release tag.

## Retry policy

Re-run the failed workflow if publication stops partway. The plan step skips
versions that already exist. Do not move a tag after an upload succeeds. If the
release commit changes, use a new patch version.

## Trusted publishing

Trusted Publishing is tied to the workflow filename `release.yml` and to an empty
GitHub environment. Update every crate's trusted publisher before renaming the
workflow or adding an environment.

## Credentials

Normal releases use OIDC. Keep a break-glass crates.io token only if needed for
first-time crates. Restrict it to publish operations and rotate it on the normal
credential schedule.
