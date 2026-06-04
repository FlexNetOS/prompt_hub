---
name: security-remediation
description: "Triggers when triaging Dependabot, cargo-audit, or cargo-deny security alerts. Encodes a reasoning-anchored remediation procedure — the counter to mechanical version bumps. Use it whenever a security alert, advisory, or RUSTSEC ID needs a fix in prompt_hub."
---

# Security Remediation Skill

## Premise

A security alert is a **claim that a vulnerable code path exists in our build**, not a
command to bump a version. Most mechanical "Dependabot bumped X" PRs are noise — they
churn the lockfile, sometimes regress behaviour, and rarely reflect whether the flaw is
even reachable. This skill is the counter: **reason about reachability first, then apply
the smallest fix that removes the path, then prove it.** Never bump silently.

Process exactly **one alert at a time** through steps 1–6. Bundle the confirmed results
into a single PR at the end.

## Step 1 — Inventory signals

Gather every source of truth before deciding anything:

```bash
gh api repos/<owner>/<repo>/dependabot/alerts            # GitHub's view
cargo audit                                              # RUSTSEC advisories vs Cargo.lock
cargo deny check                                         # advisories + licenses + bans + sources
```

Record, per alert: the crate + version, the advisory ID (RUSTSEC-/CVE-/GHSA-), the
introducing path, and what each tool says (they disagree often; that disagreement is data).

## Step 2 — REACHABILITY anchor (this decides everything)

Classify the alert into exactly one bucket. **Do not proceed to a fix until this verdict
is written down with evidence.**

| Verdict | Meaning | How to confirm |
|---|---|---|
| **reachable** | Vulnerable API is called on a live code path | `cargo tree -i <crate>` shows a non-optional path; `grep` finds call sites; `git-kb code callers <symbol>` / `git-kb code impact <symbol>` shows a chain from our public API |
| **transitive-only** | Pulled by a dep, but we never call the flawed API | `cargo tree -i` shows only transitive parents; `grep` finds no call sites |
| **feature-gated** | Only present when an optional feature / default-feature is enabled | Trace the `[features]` graph in the relevant `Cargo.toml`; check `default-features` and the optional-dep `dep:`/`?` edges |
| **unreachable** | Compiled out entirely for our feature set / target | `cargo tree -i` returns nothing for our active features; or it's a build/dev-only dep |

Tools: `cargo tree -i <crate> --all-features` vs the **actual** feature set we ship,
`grep -rn "<vulnerable_fn>" prompt-hub/src prompthub/src prompthub-server/src`,
`git-kb code callers/impact` for the call-graph, and the `[features]` tables in each
crate's `Cargo.toml` (workspace deps live in the root `Cargo.toml`).

A fix that does not change the reachability verdict is not a fix.

## Step 3 — Minimal-fix ladder (climb only as far as needed)

Pick the **lowest** rung that removes the reachable/feature-gated path. Higher rungs are
only justified when lower ones can't close it.

1. **patch-bump** — `cargo update -p <crate> --precise <ver>`. Semver-compatible, no code
   change. Preferred whenever a fixed patch/minor exists.
2. **feature-trim** — drop an unused optional feature so the vulnerable subtree is never
   compiled (`default-features = false`, then add back only what's used). Best when the
   alert is **feature-gated** and the feature is dead weight.
3. **major-migrate** — bump across a breaking boundary **and** write the code migration to
   match the new API. Only when no compatible fix exists and the path is reachable.
4. **sha-pin-or-remove** (GitHub Actions) — pin the action to a full commit SHA, or remove
   the step. Use for workflow/Action advisories.
5. **justified-suppress** — add to `.deny.toml` `[advisories].ignore` (or `cargo audit`
   ignore) **with a rationale comment AND a recheck date**. Allowed **only** when the
   verdict is **unreachable** or **feature-gated with no clean upgrade** — never for a
   reachable path.

```toml
# .deny.toml
[advisories]
ignore = [
  # RUSTSEC-YYYY-NNNN: <crate> — unreachable: pulled transitively via <parent>,
  # flawed API <fn> not called (verified cargo tree -i + grep, <date>).
  # No fixed release yet. RECHECK: 2026-09-01.
  "RUSTSEC-YYYY-NNNN",
]
```

## Step 4 — VERIFY (every fix passes the full repo gate)

Work in a **git worktree** — the pre-commit hook / `scripts/code_review.sh` refuses
commits made in the main working tree.

```bash
git worktree add ../ph-sec-<alert> -b sec/<alert>
cd ../ph-sec-<alert>
# apply the fix here, then:
bash scripts/code_review.sh        # clippy --workspace --all-features -D warnings + tests
cargo audit                        # the alert must be gone (or justified-ignored)
cargo deny check                   # advisories + licenses + bans + sources clean
cargo publish -p prompt-hub --dry-run   # the published crate must still package
```

A fix that turns the alert green but fails any of these is not done.

## Step 5 — Adversarial check (refute your own fix)

Before trusting the green gate, a second pass actively tries to break the fix:

- Does the upgrade change **auth, crypto, or semantics**? (e.g. a TLS/cert crate bump that
  silently changes verification behaviour, a default that flips, a signature/format change.)
- Does the feature-trim drop behaviour we actually rely on at runtime — including on a
  code path tests don't cover? Re-grep for the trimmed feature's API surface.
- Does a `--precise` pin pin us **below** another crate's requirement, or split versions
  (check `cargo deny check bans` for `multiple-versions`)?
- Does the suppression hide a path that becomes reachable when a downstream feature is
  enabled? Re-run reachability with `--all-features`.

If the refutation holds, climb the ladder or escalate. Do not ship a fix you couldn't break
only because you didn't try.

## Step 6 — Provenance + escalation

- Open **ONE PR** bundling all confirmed fixes. Each entry records: alert ID, the
  **reachability verdict**, the evidence (commands + output), the ladder rung used, and the
  gate results.
- Record every suppression with its rationale and **expiry date** in `.deny.toml`; track the
  recheck so it isn't permanent.
- **Escalate, never silently bump:** any alert that is not `auto_safe` — breaking major
  bumps, anything touching auth/crypto/semantics, anything where reachability is uncertain —
  goes to a human with a short summary (verdict, options, recommendation). The default for
  ambiguity is escalation, not a mechanical bump.

## Worked example — libsql feature-trim killed the rustls-webpki advisories

cargo-audit flagged `rustls-webpki` advisories in this repo. Reachability anchor: the
storage layer only ever uses `Builder::new_local` — we never open a **remote/replicated**
libsql connection, so the WebPKI cert-verification path was **feature-gated and unreachable**
for our usage. Rather than chase upstream `rustls-webpki` bumps (rung 1) or suppress (rung 5),
we took **rung 2, feature-trim**: in the root `Cargo.toml` the workspace dep became

```toml
libsql = { version = "0.9", default-features = false, features = ["core"] }
```

Dropping libsql's default features removed the remote/replication subtree that pulled
`rustls-webpki` entirely — the advisories vanished from `cargo audit`/`cargo deny check`
because the vulnerable crate is no longer compiled, not merely upgraded. The adversarial
check confirmed local storage still works (no remote API in use) and the gate
(`scripts/code_review.sh` + audit + deny + publish dry-run) passed. One verdict, one
minimal fix, proven — no version churn.
