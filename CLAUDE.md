# CLAUDE.md

Project-specific guidance for Claude Code (and any AI agent) operating
in this repository. Supplements the global rules in
`~/.claude/CLAUDE.md`; does not replace them.

## Multi-angle PR review (mandatory)

**Every pull request that Claude creates in this repository must be
reviewed by a team of 5-10 AI reviewer agents before it is merged.**
Findings are posted as comments on the PR. The lead Claude session
synthesizes them into a single consolidation comment and only then
proceeds with the merge (or amends the PR if blocking findings
surfaced).

This is a hard rule, not a suggestion. It exists because:

- This project is solo-maintained. The multi-angle review compensates
  for the absence of independent human reviewers.
- Different angles catch different classes of defect. A single
  generalist reviewer misses things that a focused security or a
  focused performance reviewer catches as a matter of habit.
- The review record on each PR becomes a durable artefact of *why*
  the decisions in that PR were made. Six months later, the synthesis
  comment is more readable than a `git blame` trail.

### Workflow

For every PR Claude creates in this repo:

1. **Create the PR** as usual: branch, commit, push, `gh pr create`.
2. **Wait for CI to pass** (`build` and `commit lint`). Don't burn
   reviewer turns on issues CI would have caught.
3. **Select 5-10 reviewer roles** based on PR context. See the
   reviewer pool and selection heuristic below. The exact count is a
   judgement call; bias toward 7 unless the PR is unusually narrow
   or unusually wide.
4. **Spawn a review team** via `TeamCreate` plus parallel `Agent`
   calls. **Team name**: `roger-pr<N>-review` where `<N>` is the PR
   number. The PR number is globally unique so concurrent review
   runs cannot collide on team-namespace. Each teammate brief must
   contain:
   - The PR number and URL.
   - The PR title and body.
   - The full diff (via `gh pr diff <num>` in their tools).
   - The linked issue body (`gh issue view <num>`).
   - Their assigned angle/role.
   - The exact format their PR comment must follow (see below).
   - The exact `gh pr comment` invocation to post it, including
     fallback if posting fails (see step 5).
5. **Each reviewer posts one PR comment** to the PR in their angle,
   using `gh pr comment <num> --repo asivura/roger --body-file <path>`.
   **Failure recovery:** if the post fails (network blip, gh auth
   issue, GitHub rate-limit), the reviewer must retry once. If the
   second attempt also fails, the reviewer reports the body content
   plus the error inline to the lead via `SendMessage`; the lead
   posts the comment on the reviewer's behalf. A missing review
   comment is a blocker for synthesis.
6. **The lead synthesizes** the findings into a single consolidation
   comment, also posted to the same PR.
7. **Decide:**
   - **Critical findings** → amend the PR (force-push). Wait for CI
     to pass again. **Re-review scope on amend**: by default only
     the originating reviewer re-checks (via a follow-up PR comment
     confirming the fix); spawn the full team for re-review only if
     the amend itself is substantive enough to warrant a fresh
     selection of angles (i.e., the amend is itself PR-shaped). The
     lead documents which path was taken in a follow-up synthesis
     comment.
   - **Important findings** → amend if cheap; otherwise open
     follow-up issues. Document the decision in the synthesis.
   - **Minor findings** → open follow-up issues by default; only
     amend if the fix is genuinely cheaper than the issue.
   - **No blocking findings** → merge via rebase
     (`gh pr merge --rebase --delete-branch`).
8. **Shut down the review team**: `SendMessage` with
   `shutdown_request` to each reviewer, wait for `shutdown_approved`,
   then `TeamDelete`.

### Reviewer pool

Pick 5-10 from this pool. Always include `security` and
`correctness`. The remaining slots are PR-context-dependent.

| Role | Pick when |
|---|---|
| `security` | always |
| `correctness` | always — focused on logic correctness, not style |
| `rust-idioms` | any Rust code change |
| `wasm-sandbox` | plugin code, build profile, target choices, dependencies that may not work in wasi |
| `zellij-api` | code that uses `zellij_tile` APIs |
| `performance` | hot-path code, build profile, CI run time, Wasm size, RPC latency |
| `architecture` | new modules, multi-file refactors, the RPC protocol |
| `backward-compat` | public RPC protocol, CLI surface, on-disk format, exported APIs |
| `error-handling` | code that handles `Result` / panics / fallible IO |
| `concurrency-state` | plugin state machine, lifecycle events, async ordering |
| `testing` | tests added/changed, or code that should have tests but doesn't |
| `github-actions-idioms` | `.github/workflows/**` changes |
| `dependencies` | `Cargo.toml`, `Cargo.lock`, action versions, Dependabot bumps |
| `developer-experience` | aliases, onboarding, error messages, CONTRIBUTING |
| `documentation` | README, ROADMAP, CONTRIBUTING, RELEASING, CHANGELOG, `docs/**` |
| `naming-consistency` | new APIs, new files, public identifier introductions |
| `commit-message-quality` | optional — CI's `commit lint` job already enforces Conventional Commits format; pick only when the PR has unusual commit-message structure (squashes, reverts, complex rebases) |
| `changelog-update` | always when the PR is user-visible (features, fixes, breaking changes, install-flow shifts); skip for pure internal refactors / CI-only changes |
| `process-meta` | PRs that touch this workflow, `CONTRIBUTING.md`, `RELEASING.md`, branch protection, CI policies |
| `community-conventions` | novel patterns — does it match what `zjctl` / `zellaude` / `zjstatus` / `zellij-attention` do? |

### Selection heuristic by PR type

| PR type | Typical roster (5-8 roles) |
|---|---|
| Code change in `plugin/` | `security`, `correctness`, `rust-idioms`, `wasm-sandbox`, `zellij-api`, `error-handling`, `testing`, `changelog-update` |
| Code change in `shim/` | `security`, `correctness`, `rust-idioms`, `error-handling`, `backward-compat`, `testing`, `changelog-update` |
| RPC protocol design | `security`, `correctness`, `architecture`, `backward-compat`, `error-handling`, `documentation`, `community-conventions` |
| CI / workflow change | `security`, `github-actions-idioms`, `performance`, `dependencies`, `developer-experience`, `documentation`, `changelog-update` |
| Documentation change | `documentation`, `developer-experience`, `naming-consistency`, `process-meta` (governance docs), `changelog-update`, `security` (if it includes install instructions, especially `curl ... | sh`) |
| Dependency bump | `security`, `dependencies`, `backward-compat`, `testing`, `changelog-update` |
| Refactor (no behavior change) | `correctness`, `rust-idioms`, `architecture`, `naming-consistency`, `backward-compat`, `testing`, `changelog-update` |
| Workspace / build layout | `correctness`, `rust-idioms`, `architecture`, `developer-experience`, `documentation`, `community-conventions`, `changelog-update` |

**Default sizing.** If the PR fits one row cleanly, use that row's
roster verbatim (rows are 5-8 roles; one row sets the count). If the
PR is unusually narrow (a Cargo.lock-only bump, a typo fix) and the
matching row has fewer than 5 roles, keep the smaller roster rather
than padding to 5 with low-signal angles. The 5-10 range is a
sanity-check guardrail, not a target.

**Multi-domain PR composition.** When a PR spans multiple rows (e.g.
plugin code + CI change in one PR), union the matching rosters,
dedupe by role, and cap at 10. If the dedupe-union exceeds 10, drop
the roles with the lowest signal for *this specific PR* (the lead's
judgement; document the drop in the team's `description`).

**Always-include floor.** `security` and `correctness` are in every
roster above; if you build a custom roster, include them. Other
roles in the pool are picked when triggered.

### Reviewer comment format

Each reviewer posts exactly one PR comment, formatted as:

```markdown
## Review: <role>

### Critical
<!-- must-fix before merge; or "None" -->

### Important
<!-- should-fix; either amend or follow-up; or "None" -->

### Minor
<!-- nice-to-have; usually follow-up; or "None" -->

### Praise
<!-- what this PR gets right; concrete callouts, not flattery; or "None" -->

### Summary

<1-2 sentences capturing the reviewer's overall read>
```

Empty buckets are written as "None" so emptiness is explicit
information rather than absent information.

Each finding (in Critical / Important / Minor) has the shape:

1. **<one-line summary>** — `<file>:<line>` (when applicable)
   <2-4 sentence rationale>
   *Recommendation:* <concrete action the maintainer can take>

### Synthesis comment format

The lead posts exactly one synthesis comment after all reviewers have
reported:

```markdown
## Multi-angle review synthesis

**Reviewers** (N): `<role1>`, `<role2>`, ..., `<roleN>`.

### Critical findings
- [<role>] <finding> — *decision:* <amend in this PR / block merge>

### Important findings
- [<role>] <finding> — *decision:* <amend / follow-up #N>

### Minor findings
- [<role>] <finding> — *decision:* <follow-up #N / accept as-is>

### Praise highlights
- [<role>] <what this PR gets right>

### Decision

<Merge as-is via rebase / amend then merge / hold pending #N>
```

### When this workflow can be skipped

By default, **never**. The rule applies to:

- Maintainer-authored PRs (Claude's own).
- Dependabot bumps (still need `security`, `backward-compat`,
  `dependencies`, optionally `testing` and `changelog-update`).
- Trivial typo fixes (still need `documentation`,
  `naming-consistency`, `security` if onboarding-adjacent).
- Emergency hotfixes (a smaller team — minimum 3 — is acceptable only
  if the maintainer explicitly approves the abbreviated review in
  the PR body).

If you genuinely think a PR should not get a multi-angle review,
write the reasoning in the PR body and proceed only after the user
(maintainer) explicitly approves the skip.

### Honesty about AI-reviewing-AI

There is a real failure mode where AI reviewers reflexively endorse
AI-authored PRs. Mitigate by:

- Giving each reviewer their angle in isolation (no shared deliberation
  before they report); this reduces groupthink.
- Asking each reviewer to write a Praise section that names *what
  specifically this PR gets right* — flattery is forbidden, only
  concrete callouts. If a reviewer cannot produce a concrete Praise
  bullet, that itself is informative.
- Treating "no critical findings" as data, not vindication. If five
  reviewers in a row report zero findings on real code changes, the
  brief is probably too narrow; widen the angles.

### Cost notes

A 5-reviewer team takes ~5-15 minutes wall-clock with the in-process
backend. A 10-reviewer team takes ~10-20 minutes. The lead session
should coordinate and synthesize, not run reviews itself; doing both
inflates the lead's context.

**Worked dollar estimate** (rough, depends on model choice):

| Model | Per reviewer (~50k in + 2k out) | 7-reviewer team + synthesis |
|---|---|---|
| Sonnet | ~$0.18 | ~$1.50 |
| Opus | ~$0.90 | ~$7-8 |

At 5 PRs/day (an unusually busy day for this repo), that's roughly
$8-50/day depending on model. At more realistic 1-2 PRs/week,
$15-65/month. Treat this as the quality cost of solo-maintainer OSS;
the [follow-up issue](https://github.com/asivura/roger/labels/area%3A%20process)
on cost optimizations covers cadence-batching and synthesis-teammate
delegation if the bill grows.

### Origin and posture

The workflow codified here is **stricter than the practice observed
in PR #17** (the first multi-angle review on this repo, retrospective
and lead-chat-only — zero PR comments). PR #29 introduces:

- **Pre-merge gating** instead of post-merge retrospective.
- **PR-comment delivery** instead of lead-chat-only findings.
- **Synthesis posted to the PR** instead of just the lead's chat.

These shifts are deliberate: they create a durable record on each PR
and force the reviewer findings to be human-readable (a future
contributor can read PR comments without access to the maintainer's
Claude session transcript). If the new mandates prove too costly in
practice, the right response is to loosen them deliberately via an
amended CLAUDE.md, not to silently revert to PR #17 patterns.

## Other project conventions

For Conventional Commits, branch naming, rebase-only merge style,
build commands, and the SemVer release flow, see
[CONTRIBUTING.md](CONTRIBUTING.md) and [RELEASING.md](RELEASING.md).

For the implementation roadmap and current phase status, see
[ROADMAP.md](ROADMAP.md).
