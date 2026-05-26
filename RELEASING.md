# Releasing roger

Step-by-step for cutting a new release. See [CONTRIBUTING.md](CONTRIBUTING.md)
for the versioning policy.

## Prerequisites

- You are on `main`, up to date with `origin/main`, working tree clean.
- All milestone issues for the release are closed.
- CI on the latest `main` commit is green.

## Procedure

1. **Pick the version**. Use [SemVer](https://semver.org/spec/v2.0.0.html):
   MAJOR for breaking RPC / shim / install changes, MINOR for new
   features, PATCH for bug fixes only.

2. **Open a release-prep PR.** Branch name: `chore/release-vX.Y.Z`.

3. **Bump the version** in [`Cargo.toml`](Cargo.toml) and run
   `cargo build-wasm` so `Cargo.lock` updates with the new version
   metadata.

4. **Update [`CHANGELOG.md`](CHANGELOG.md)**:

   - Move all entries from `## [Unreleased]` into a new
     `## [X.Y.Z] - YYYY-MM-DD` section above it. Use today's date in
     UTC.
   - Add a fresh empty `## [Unreleased]` section at the top with
     empty `### Added`, `### Changed`, `### Fixed` placeholders
     (delete unused sections at release time).
   - Update the link references at the bottom of the file:
     `[Unreleased]: .../compare/vX.Y.Z...main` and
     `[X.Y.Z]: .../compare/vPREV...vX.Y.Z`.

5. **Commit, push, open PR.** Commit message:
   `chore(release): vX.Y.Z`.

6. **Multi-angle review then merge via rebase** once CI is green.
   Release PRs are not exempt from the review mandate in
   [CLAUDE.md](CLAUDE.md); use a slim roster (`security`,
   `correctness`, `changelog-update`, `release-engineering` once
   that role lands, `documentation`) since the diff is typically
   small. Merge via rebase after the synthesis comment lands.

7. **Sync local main and tag**:

   ```bash
   git checkout main
   git pull --ff-only origin main
   git tag -a vX.Y.Z -m "vX.Y.Z"
   git push origin vX.Y.Z
   ```

8. **Build the release artifact locally**:

   ```bash
   cargo build-wasm
   # artifact at: target/wasm32-wasip1/release/roger.wasm
   ```

9. **Create the GitHub Release**:

   ```bash
   gh release create vX.Y.Z \
     --title "vX.Y.Z" \
     --notes-from-tag \
     target/wasm32-wasip1/release/roger.wasm
   ```

   Or use `--notes-file <path>` with a release-notes file derived
   from the CHANGELOG section.

10. **Announce / update external references** (when relevant):

    - Submit a PR to [`zellij-org/awesome-zellij`](https://github.com/zellij-org/awesome-zellij)
      adding `roger` if not yet listed (only for v0.1.0).
    - Bump any pin in dependent projects.

## Rollback

If a release tag turns out to be broken:

- Do not retag the same version. Cut `vX.Y.Z+1` with the fix.
- If the release is genuinely unusable, mark the GitHub Release as
  a pre-release (`gh release edit vX.Y.Z --prerelease`) and link to
  the fix in the release notes.

## Yanking a crates.io release

Not yet applicable; we do not currently publish to crates.io
(plugins are loaded as `.wasm` artifacts, not as cargo
dependencies). Revisit when the shim crate is published.
