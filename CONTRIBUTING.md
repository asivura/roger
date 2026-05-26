# Contributing to roger

Thanks for considering a contribution. This document captures the
conventions that keep the project tractable for a small maintainer
team. Most of them are mechanical (commit format, branch names);
follow them and the work flows smoothly.

## Code of conduct

Be kind, assume good faith, and stay on topic. We follow the
spirit of the [Contributor Covenant](https://www.contributor-covenant.org/)
without the ceremony of formally adopting it as a separate document.

## Toolchain setup

The repo ships a `rust-toolchain.toml` that pins `channel = "stable"`
and `targets = ["wasm32-wasip1"]`. Running any `cargo` command in
the repo for the first time will trigger `rustup` to install the
toolchain and the wasm target automatically. No manual `rustup`
invocations needed.

If you don't have `rustup` yet:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

## Build and verify

The repo's `.cargo/config.toml` defines four aliases that match what
CI runs. Two per crate (plugin = Wasm target, shim = host target):

```bash
# Plugin (the Wasm artifact)
cargo build-wasm    # release build  -> target/wasm32-wasip1/release/roger.wasm
cargo check-all     # clippy with -D warnings

# Shim (the host-target `tmux`-shaped binary)
cargo build-shim    # release build  -> target/release/tmux
cargo check-shim    # clippy with -D warnings
```

Also useful directly:

```bash
cargo fmt --all              # format
cargo fmt --all -- --check   # check formatting (what CI does)
cargo test                   # run tests (once any exist)
```

**Verify before pushing**: run `cargo fmt --all -- --check`,
`cargo check-all`, `cargo check-shim`, `cargo build-wasm`, and
`cargo build-shim` locally. If all five are clean, CI will be clean.

## Branch naming

`<type>/<short-description>`, where `<type>` is one of the
[Conventional Commits](#conventional-commits) types below. Examples:

- `feat/team-spawn-rpc`
- `fix/scaffold-permission-name`
- `docs/install-guide`
- `ci/dependabot-config`
- `chore/bump-deps`
- `refactor/workspace-restructure`

## Conventional commits

Every commit message must follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <subject>

<optional body, wrapped sensibly>

<optional footer, e.g. "Closes #N">
```

CI enforces the type via the `commit lint` job
([`.commitlintrc.yml`](.commitlintrc.yml) extends
`@commitlint/config-conventional`). **The full list of accepted
types** is:

| Type | Use for |
|---|---|
| `feat` | A new feature visible to users |
| `fix` | A bug fix |
| `docs` | Documentation only |
| `style` | Formatting, whitespace, missing semis (no behavior change) |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or correcting tests |
| `build` | Build system, Cargo dependencies, release profile |
| `ci` | CI configuration, workflow files |
| `chore` | Maintenance work that doesn't fit elsewhere |
| `revert` | Revert of a prior commit |

**Common mistake**: types not on this list (`dx`, `wip`, `tweak`,
`misc`, etc.) will fail the commit-lint check. When unsure, use
`chore`.

**Subject line**: max 72 characters, imperative mood, no trailing
period, no capital after the colon. Good:
`feat(plugin): implement team.spawn RPC`. Bad:
`Feat(plugin): Implemented team.spawn.`.

**Body**: free-form. Body line length is intentionally not capped
(see [`.commitlintrc.yml`](.commitlintrc.yml)) so you can paste
`cargo` output, stack traces, or `git diff` excerpts without
fighting wrap rules.

**Breaking changes**: add `!` after the type. Example:
`feat!: drop support for zellij-tile 0.43`. Document the break in
the body.

### Fixing a rejected commit message

If `commit lint` fails on your PR, the fix is to rewrite the offending
commit message and force-push the branch:

```bash
# For the most recent commit
git commit --amend
# edit the message, save, exit

# For an older commit on the branch
git rebase -i HEAD~N    # change 'pick' to 'reword' for the bad one

# Push the rewritten history
git push --force-with-lease
```

`--force-with-lease` is safer than `--force`: it refuses to push if
someone else has pushed to the branch since your last fetch.

## Pull requests

1. Create a branch off the latest `main`.
2. Make focused commits using the conventions above.
3. Before opening: rebase onto `main`, run `cargo check-all` and
   `cargo build-wasm` locally.
4. Open the PR via `gh pr create` or the web UI. Fill out the
   [PR template](.github/PULL_REQUEST_TEMPLATE.md).
5. CI runs `build` and `commit lint`. Both must pass.
6. After review (or for solo maintainer work, after CI is green):
   merge via **rebase**. The repo is configured to allow only
   "Rebase and merge" in the GitHub UI; squash and merge-commit
   styles are disabled.

If `main` advances while your PR is open, rebase your branch onto
`main` before merging. Branch protection requires the branch to be
up-to-date with `main` for the merge button to enable.

## Versioning

The project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

- **MAJOR** (`1.0.0` → `2.0.0`): breaking changes to the plugin's
  RPC protocol, shim CLI compatibility, or install flow.
- **MINOR** (`0.1.0` → `0.2.0`): new features, backwards compatible.
- **PATCH** (`0.1.0` → `0.1.1`): bug fixes only.

The `version` field in [`Cargo.toml`](Cargo.toml) is the canonical
source of truth.

See [`RELEASING.md`](RELEASING.md) for the cut-a-release procedure.

## Where to start

- New contributors: look for issues tagged
  [`good first issue`](https://github.com/asivura/roger/issues?q=is%3Aopen+label%3A%22good+first+issue%22).
- The full implementation plan is in [`ROADMAP.md`](ROADMAP.md);
  issues are grouped by phase via
  [GitHub Milestones](https://github.com/asivura/roger/milestones).
