# Releasing

The release workflow publishes six crates in dependency order:

1. `adotenv`
2. `adotenv-lint`
3. `adotenv-process`
4. `adotenv-config`
5. `adotenv-figment`
6. `adotenv-cli`

## Versions

`adotenv`, `adotenv-process` and `adotenv-cli` share one version. The release
tag `vX.Y.Z` must match it.

`adotenv-lint` has its own 0.1.x line. `adotenv-config` and `adotenv-figment`
have their own 0.x versions, because their public API implements traits from
`config` and `figment`. A new major of either dependency is a minor release of
the adapter. The workflow publishes an independent crate whenever its declared
version is not on crates.io yet. Bump an adapter or the linter when its code,
its third-party dependency or its `adotenv` requirement changes. Changing the
`adotenv` major version requirement is a breaking change for dependents.

The seven published crates are registered with Trusted Publishing. They publish
through GitHub Actions OIDC. `adotenv-lint` 0.1.0 was first published manually
on 2026-11-06; 0.1.1 was its first OIDC release. Its Trusted Publisher is
configured for this repository, workflow `release.yml` and no environment.

`adotenv-lsp` 0.1.0 was first published on 2026-11-07 through the
`publish_lsp=true` workflow dispatch, using a temporary bootstrap token. The
token secret was removed and the token revoked, and its Trusted Publisher is
registered for this repository, workflow `release.yml` and no environment.
Future LSP releases use `publish_lsp=true` with OIDC.

### LSP releases

For each later `adotenv-lsp` version:

1. Freeze the public API for the intended 0.x line and document remaining
   prototype limitations.
2. Add an `## [adotenv-lsp X.Y.Z] - YYYY-MM-DD` changelog section rather than
   relying on the core release entry.
3. Update the `adotenv` and `adotenv-lint` dependency constraints if needed.
4. Run `cargo package -p adotenv-lsp --locked` and
   `cargo publish --dry-run -p adotenv-lsp --locked`.
5. Dispatch `release.yml` with `publish_lsp=true`. It runs the quality gate,
   checks the version is unpublished and publishes after the core dependencies.
6. Verify the new crates.io and docs.rs pages.

## Preflight

1. Run the CI gate locally. The authoritative list of commands is the CI
   workflow, [`ci.yml`](.github/workflows/ci.yml);
   [quality.md](docs/design/quality.md) summarizes it. A manual dispatch of the
   release workflow runs the same gate in CI without checking versions or
   publishing.
2. Confirm the lockstep crates use the same version and that every path
   dependency on `adotenv` requires that version.
3. The CI `semver` job checks each library crate against its latest
   published version on every pull request, push to `main` and release tag.
   As a cross-check, run it locally with the intended release type:

   ```console
   cargo semver-checks check-release -p adotenv --baseline-version <previous-version> --release-type <patch|minor|major>
   ```

   Repeat for `adotenv-process`, `adotenv-config`, `adotenv-figment` and
   `adotenv-lint`. Review any failure before release.

4. Update package manifests and path dependency constraints explicitly. Do not
   replace every matching version string in `Cargo.lock`. Third-party crates can
   share the same version number. The conformance crate has its own lock file,
   `conformance/rust/Cargo.lock`; update the `adotenv` version there too,
   because CI builds that crate with `--locked`.
5. Move the `Unreleased` entries in `CHANGELOG.md` into a section headed
   `## [X.Y.Z] - YYYY-MM-DD` for the lockstep version, and update the version in
   the README and docs install snippets.

## Publish

Create and push an annotated tag from the release commit:

```console
git tag -a vX.Y.Z -m "adotenv X.Y.Z"
git push origin vX.Y.Z
```

The workflow:

- runs the quality gate;
- checks that the tag matches the lockstep version;
- skips versions already present on crates.io;
- uses OIDC for existing crates;
- waits for each version in the sparse index before publishing dependents;
- creates the GitHub Release from the matching `CHANGELOG.md` section.

For a linter-only release without a core tag, dispatch the same workflow with
`publish_linter=true`. It runs the quality gate, verifies the current linter
version is unpublished, and publishes it through OIDC.

For an LSP release, dispatch the same workflow with `publish_lsp=true`; it uses
OIDC.

## Verify

Check the published crate pages:

- <https://crates.io/crates/adotenv>
- <https://crates.io/crates/adotenv-process>
- <https://crates.io/crates/adotenv-config>
- <https://crates.io/crates/adotenv-figment>
- <https://crates.io/crates/adotenv-cli>
- <https://crates.io/crates/adotenv-lint>
- <https://crates.io/crates/adotenv-lsp>

Check the docs.rs builds of the six library crates. `adotenv-cli` is a binary
crate and has no library documentation. Confirm that the GitHub Release points
at the release tag.

## Retry policy

Re-run the failed workflow if publication stops partway. The plan step skips
versions that already exist, and the GitHub Release job skips an existing
release. Do not move a tag after an upload succeeds. If the release commit
changes, use a new patch version.

## Trusted publishing

Trusted Publishing is tied to the workflow filename `release.yml` and to an empty
GitHub environment. Update every crate's trusted publisher before renaming the
workflow or adding an environment.

## New crates

crates.io cannot register a trusted publisher for a crate that does not exist
yet, so the first publish of `adotenv-lint` or any other new crate uses a
token:

1. Create a crates.io API token limited to `publish-new` for the new crate name,
   with a short expiry.
2. Add it as the repository secret `CARGO_REGISTRY_TOKEN`.
3. Release as usual. The plan step uses the token only for crates that are not
   on crates.io yet; existing crates still publish through OIDC.
4. On crates.io, add the trusted publisher for the new crate: this repository,
   workflow `release.yml`, no environment.
5. Delete the secret and revoke the token.

## Credentials

Normal releases use OIDC. Keep a break-glass crates.io token only if needed for
first-time crates. Restrict it to publish operations and rotate it on the normal
credential schedule.
