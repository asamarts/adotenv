# Releasing adotenv

The release workflow (`.github/workflows/release.yml`) publishes all five
workspace packages in dependency order:

1. `adotenv`
2. `adotenv-process`
3. `adotenv-config`
4. `adotenv-figment`
5. `adotenv-cli`

The 0.7.0 bootstrap is complete: all five crates are published and their
Trusted Publishing configurations point at this repository, `release.yml`, and
no environment. The 0.7.1, 0.8.0, 0.9.0, 0.9.1, 0.9.2, 0.9.3, 0.9.4, 0.9.5, 0.9.6, 0.9.7, 0.9.8, 0.9.9, 0.9.10, 0.9.11 and 0.9.12 releases were then published keylessly
through those trusted publishers. The temporary token was retained intentionally as a
fallback for future first-time crates.

A tag push runs the full CI quality gate first. The workflow then:

- verifies that the tag matches every package version;
- skips versions already present on crates.io, so a failed run can be retried;
- uses Trusted Publishing/OIDC for crates that already exist;
- waits for each version to appear in the crates.io sparse index before
  publishing a dependent crate.

## Pre-tag API check

Before tagging a core release, compare the current tree against the previous
published core version even when the next version is still pre-1.0:

```console
cargo install cargo-semver-checks --locked
cargo semver-checks check-release --baseline-version <previous-version> --release-type patch
```

`--release-type patch` forces the complete check set instead of skipping checks
for a pre-1.0 minor bump. Intentional API changes must be reviewed against the
changelog; accidental breakage should not be released just because semver allows
it before 1.0.

When bumping workspace versions, update package manifests and path dependency
requirements explicitly. Do not blanket-replace every version string in
`Cargo.lock`: third-party dependencies can share the old version number (for
example `version_check`), and rewriting them corrupts resolution.

## One-time bootstrap for new crates

crates.io does not support a trusted publisher for a crate that does not exist
yet. The first publish of each new crate therefore needs a temporary API token.
For 0.7.0 this applied to `adotenv-process`, `adotenv-cli`, `adotenv-config`
and `adotenv-figment`; all five trusted publishers are now configured.

1. Confirm that the crates.io account has a verified email address and owns
   `adotenv`.
2. Confirm the four new names are still available:
   <https://crates.io/crates/adotenv-process>,
   <https://crates.io/crates/adotenv-cli>,
   <https://crates.io/crates/adotenv-config> and
   <https://crates.io/crates/adotenv-figment>.
3. Create a narrow crates.io API token with `publish-new` and
   `publish-update` permissions.
4. Add it as the repository secret `CARGO_REGISTRY_TOKEN` in
   **Settings → Secrets and variables → Actions**.
5. Push the release tag from the release commit. Substitute the current
   version; the example below is the historical 0.7.0 bootstrap:

   ```console
   git tag -a vX.Y.Z -m "adotenv X.Y.Z"
   git push origin vX.Y.Z
   ```

6. Watch the **Release** workflow. It uses OIDC for the existing `adotenv`
   crate and the temporary token for the four first-time companions.
7. Verify all five crates and docs builds:

   - <https://crates.io/crates/adotenv/0.7.0>
   - <https://crates.io/crates/adotenv-process/0.7.0>
   - <https://crates.io/crates/adotenv-cli/0.7.0>
   - <https://crates.io/crates/adotenv-config/0.7.0>
   - <https://crates.io/crates/adotenv-figment/0.7.0>
   - <https://docs.rs/adotenv/0.7.0/>
   - <https://docs.rs/adotenv-process/0.7.0/>
   - <https://docs.rs/adotenv-cli/0.7.0/>
   - <https://docs.rs/adotenv-config/0.7.0/>
   - <https://docs.rs/adotenv-figment/0.7.0/>

8. For **each** new crate, open its crates.io settings and add a trusted
   publisher:

   - Provider: GitHub Actions
   - Repository owner: `asamarts`
   - Repository name: `adotenv`
   - Workflow filename: `release.yml`
   - Environment: leave empty

   Optionally enable “Trusted Publishing Only” after the configuration works.

9. The temporary token is no longer required once all four crates exist.
   Delete it if you do not need a break-glass credential. If you intentionally
   retain it, restrict it to `publish-new`/`publish-update`, protect the
   repository secret and rotate it on your normal credential schedule. The
   workflow uses OIDC for existing crates and uses the token only for a
   first-time crate.

## If a release fails partway

Re-run the failed **Release** workflow from GitHub Actions. The plan step checks
each crate/version independently:

- already-published versions are skipped;
- existing crates continue through OIDC;
- any remaining first-time crate still uses the temporary token;
- each successful publish waits for the sparse index before the next crate.

Do not move a tag after an upload has succeeded. If the release commit changes,
use a new patch version rather than re-pointing the tag. If a first upload
failed before anything was accepted, the version remains available.

## Changing the publishing model

The trusted-publisher configuration is tied to the workflow filename
`release.yml` and to the absence of an environment. Do not rename the workflow
or add a GitHub environment without updating the trusted-publisher settings for
every crate first. `adotenv` already uses the same workflow filename, so keeping
it stable avoids breaking the existing keyless release path.

GitHub Release notes and release assets are currently created after the
crates.io publication as a separate manual step.
