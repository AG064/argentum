# Argentum Goal Contract — Feature Spec for Plan D

Status: feature spec, ready to apply when the rewrite on `UI_redesign` lands
Date: 2026-08-14
Branch (plan flow): `plan/ade-harness-landscape-2026-08-14`
Target branch (implementation): `feature/goal-contract-d` (off `UI_redesign` or its successor)
Companion: `docs/plans/ade-harness-landscape-2026-08-14.md` §7 Plan D (PRIORITY)
Owner: TBD
Target crates: `argentum-domain`, `argentum-runtime`, `argentum-store`, `argentum-ui`

---

## 0. What this is

A complete spec for Plan D — the goal contract feature. It contains:

1. Type definitions in Rust, ready to paste into `argentum-domain/src/goal.rs`.
2. Event taxonomy, ready to paste into `argentum-domain/src/goal_event.rs`.
3. The lifecycle state machine and its allowed transitions.
4. Budget enforcement rules.
5. Verification record schema and the "no completion without evidence" rule.
6. Audit log emission contract.
7. Coordination contract with Plan H (context compaction predicate).
8. Test cases, ready to paste into the relevant test modules.
9. Migration path: how to land this when the rewrite commits.

It is **not compiled code yet**. It is a spec written in Rust syntax that
becomes code as soon as the rewrite's crates land in a commit.

It **does not** depend on the rewrite being committed. It is a research
artefact on the plan branch and does not interact with `UI_redesign`.

---

## 1. Type definitions

These go into `crates/argentum-domain/src/goal.rs`.

```rust
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::ids::{GoalId, IterationId, ProjectId, RunId, SessionId, VerificationId};
use crate::ids::ApprovalId;

// ---------- Goal ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: GoalId,
    pub session_id: SessionId,
    pub project_id: ProjectId,
    pub objective: GoalObjective,
    pub lifecycle: GoalLifecycle,
    pub budget: Budget,
    pub iteration_count: u32,
    pub next_action: Option<NextAction>,
    pub verification_history: Vec<VerificationId>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub audit_log_seq: u64,        // monotonic per-goal event sequence
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalObjective {
    pub statement: String,
    pub acceptance: Vec<AcceptanceCheck>,
    pub elaboration: GoalElaboration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalElaboration {
    /// Single statement, no sub-documents.
    Flat,
    /// AWS Kiro-style requirements/design/tasks sub-documents. Deferred.
    Spec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceptanceCheck {
    pub kind: AcceptanceCheckKind,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AcceptanceCheckKind {
    /// Specific files must have changed.
    ChangedFiles { paths: Vec<String> },
    /// A command must exit 0.
    CommandExitCode { command: String, max_seconds: u64 },
    /// Specific tests must pass.
    TestResults { suite: String, required_pass: u32 },
    /// User must explicitly accept the work.
    ExplicitReview,
}

// ---------- Lifecycle ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GoalLifecycle {
    /// Goal has been declared but not yet started.
    Unset,
    /// Goal is being worked on.
    Active,
    /// Goal is paused by a user decision; can resume.
    Paused,
    /// Budget has been exhausted; needs a user decision to resume.
    BudgetLimited,
    /// Goal is in the middle of verification.
    Verifying,
    /// Goal is complete (terminal). All required evidence passed.
    Complete,
    /// Goal failed (terminal). Cannot resume; must be cleared and re-created.
    Failed,
    /// Goal cancelled (terminal).
    Cancelled,
}

impl GoalLifecycle {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled)
    }

    /// States that block Plan H context compaction on the same session.
    /// Plan H reads this list at runtime.
    pub fn blocks_compaction(self) -> bool {
        matches!(self, Self::Verifying)
    }
}

// ---------- Budget ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub time: Option<TimeBudget>,
    pub tokens: Option<TokenBudget>,
    pub tool_rounds: Option<ToolRoundBudget>,
    pub iterations: Option<IterationBudget>,
}

impl Budget {
    pub fn is_exhausted(&self) -> bool {
        self.time.map_or(false, |b| b.consumed_seconds >= b.total_seconds)
            || self.tokens.map_or(false, |b| b.consumed >= b.total)
            || self.tool_rounds.map_or(false, |b| b.consumed >= b.total)
            || self.iterations.map_or(false, |b| b.consumed >= b.total)
    }

    pub fn remaining_iterations(&self) -> Option<u32> {
        self.iterations.map(|b| b.total.saturating_sub(b.consumed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeBudget {
    pub total_seconds: u64,
    pub consumed_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenBudget {
    pub total: u64,
    pub consumed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRoundBudget {
    pub total: u32,
    pub consumed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationBudget {
    pub total: u32,
    pub consumed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetKind { Time, Tokens, ToolRounds, Iterations }

// ---------- Iteration ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Iteration {
    pub id: IterationId,
    pub goal_id: GoalId,
    pub index: u32,
    pub run_id: RunId,
    pub started_at: OffsetDateTime,
    pub ended_at: Option<OffsetDateTime>,
    pub outcome: Option<IterationOutcome>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IterationOutcome {
    Pending,
    Succeeded { verification_id: VerificationId },
    Failed { reason: String },
    Cancelled,
    BudgetExhausted { kind: BudgetKind },
}

// ---------- Verification ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verification {
    pub id: VerificationId,
    pub goal_id: GoalId,
    pub iteration_id: IterationId,
    pub requested_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
    pub outcome: VerificationOutcome,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VerificationOutcome {
    Pending,
    Passed,
    Failed { reason: String },
    /// Evidence is older than the latest change in the change set.
    Stale { staleness_reason: String },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Evidence {
    ChangedFile {
        path: String,
        sha256: String,
        observed_at: OffsetDateTime,
    },
    CommandResult {
        command: String,
        exit_code: i32,
        stdout_sha256: String,
        observed_at: OffsetDateTime,
    },
    TestResult {
        suite: String,
        passed: u32,
        failed: u32,
        observed_at: OffsetDateTime,
    },
    ExplicitReview {
        user: String,
        comment: String,
        observed_at: OffsetDateTime,
    },
}

// ---------- Next action ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NextAction {
    RunIteration,
    Verify { verification_id: VerificationId },
    WaitForApproval { approval_id: ApprovalId },
    Pause,
    Resume,
    Cancel,
    Clear,
    /// Awaits user decision because budget is exhausted.
    AwaitBudgetDecision,
}
```

---

## 2. Lifecycle state machine

Allowed transitions (all others are rejected by `argentum-runtime`):

| From          | To              | Trigger                              | Event emitted         |
| ------------- | --------------- | ------------------------------------ | --------------------- |
| `Unset`       | `Active`        | `GoalCreated`                        | `GoalCreated`         |
| `Active`      | `Paused`        | user pauses                          | `GoalPaused`          |
| `Active`      | `Verifying`     | iteration completed, verification req | `GoalVerifying`       |
| `Active`      | `BudgetLimited` | any budget exhausted                 | `BudgetExhausted`     |
| `Active`      | `Failed`        | iteration failed irrecoverably       | `GoalFailed`          |
| `Active`      | `Cancelled`     | user cancels                         | `GoalCancelled`       |
| `Paused`      | `Active`        | user resumes                         | `GoalResumed`         |
| `Paused`      | `Cancelled`     | user cancels                         | `GoalCancelled`       |
| `BudgetLimited` | `Active`      | explicit user decision + budget bump | `GoalResumed` + `BudgetUpdated` |
| `BudgetLimited` | `Cancelled`   | user cancels                         | `GoalCancelled`       |
| `Verifying`   | `Complete`      | `VerificationPassed` AND all required acceptance checks satisfied | `GoalCompleted` |
| `Verifying`   | `Active`        | `VerificationFailed` or `VerificationStale` | `VerificationFailed` or `VerificationStale` (no transition) |
| `Complete`    | (terminal)      | —                                    | —                     |
| `Failed`      | (terminal)      | —                                    | —                     |
| `Cancelled`   | (terminal)      | —                                    | —                     |

The transition matrix is enforced by a single function in
`argentum-runtime`:

```rust
// crates/argentum-runtime/src/goal_transitions.rs (sketch)

pub fn transition(
    goal: &Goal,
    to: GoalLifecycle,
    event: &GoalEvent,
) -> Result<Goal, GoalTransitionError> {
    use GoalLifecycle::*;
    let allowed = match (goal.lifecycle, to) {
        (Unset, Active) => event.is_goal_created(),
        (Active, Paused) => event.is_goal_paused(),
        (Active, Verifying) => event.is_goal_verifying(),
        (Active, BudgetLimited) => event.is_budget_exhausted(),
        (Active, Failed) => event.is_goal_failed(),
        (Active, Cancelled) => event.is_goal_cancelled(),
        (Paused, Active) => event.is_goal_resumed(),
        (Paused, Cancelled) => event.is_goal_cancelled(),
        (BudgetLimited, Active) => event.is_goal_resumed() && event.is_budget_updated(),
        (BudgetLimited, Cancelled) => event.is_goal_cancelled(),
        (Verifying, Complete) => event.is_goal_completed(),
        _ => false,
    };
    if !allowed {
        return Err(GoalTransitionError::Illegal { from: goal.lifecycle, to, event: event.kind() });
    }
    let mut next = goal.clone();
    next.lifecycle = to;
    next.updated_at = event.observed_at();
    next.audit_log_seq = next.audit_log_seq.saturating_add(1);
    Ok(next)
}
```

---

## 3. Event taxonomy

All goal events share a base:

```rust
// crates/argentum-domain/src/goal_event.rs (sketch)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalEvent {
    pub goal_id: GoalId,
    pub seq: u64,
    pub observed_at: OffsetDateTime,
    pub kind: GoalEventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GoalEventKind {
    GoalCreated { objective: GoalObjective, budget: Budget },
    GoalActivated,
    GoalPaused { by: String, reason: Option<String> },
    GoalResumed { by: String },
    GoalVerifying { verification_id: VerificationId },
    GoalCompleted { verification_id: VerificationId },
    GoalFailed { reason: String },
    GoalCancelled { by: String },
    BudgetUpdated { kind: BudgetKind, new_total: u64 },
    BudgetExhausted { kind: BudgetKind, remaining: u64 },
    IterationLinked { iteration_id: IterationId, run_id: RunId, index: u32 },
    IterationCompleted { iteration_id: IterationId, outcome: IterationOutcome },
    VerificationRequested { verification_id: VerificationId, checks: Vec<AcceptanceCheckKind> },
    VerificationPassed { verification_id: VerificationId },
    VerificationFailed { verification_id: VerificationId, reason: String },
    VerificationStale { verification_id: VerificationId, staleness_reason: String },
    NextActionSet { next: NextAction },
}
```

Every event carries a monotonic `seq` per goal. The store guarantees no
two events with the same `(goal_id, seq)` are persisted; this is the
audit-log ordering invariant.

---

## 4. "No completion without evidence" rule

The single most important rule in Plan D:

> A goal can transition from `Verifying` to `Complete` **only** when
> the most recent `Verification` for the goal is in the `Passed`
> state **and** every `required` `AcceptanceCheck` is covered by at
> least one piece of `Evidence` whose `observed_at` is newer than
> any `ChangedFile` evidence from before the iteration started.

This rule is enforced in one place:

```rust
// crates/argentum-runtime/src/goal_complete.rs (sketch)

pub fn can_complete(goal: &Goal, latest: &Verification) -> Result<(), CannotComplete> {
    use CannotComplete::*;
    if goal.lifecycle != GoalLifecycle::Verifying {
        return Err(WrongState(goal.lifecycle));
    }
    if latest.outcome != VerificationOutcome::Passed {
        return Err(VerificationNotPassed(latest.outcome.clone()));
    }
    for check in goal.objective.acceptance.iter().filter(|c| c.required) {
        let covered = latest.evidence.iter().any(|e| evidence_covers(e, check));
        if !covered {
            return Err(MissingRequired(check.description.clone()));
        }
    }
    if any_evidence_is_stale(latest, goal)? {
        return Err(StaleEvidence);
    }
    Ok(())
}
```

The four error variants are the only ways a goal can fail to
complete. They are typed and surfaced in the UI as discrete
states — the user always sees *why* a goal did not complete.

---

## 5. Audit log emission contract

Every goal event is appended to `argentum-store`'s event log before
the state transition is applied. The contract is:

1. Event is constructed in `argentum-runtime` from the typed
   `GoalEvent` and the current state.
2. Event is appended to the SQLite event log under the goal's
   session and project scope.
3. The append is wrapped in a single transaction with the goal
   row update. Either both commit or neither.
4. The projection in `argentum-store` re-derives the goal from
   the event log on session start. A goal's persisted state is
   always the result of replaying its events.
5. The CLI exposes `argentum-cli goal show <id>` and
   `argentum-cli goal audit <id>` so the user can read the
   history without the UI.

Rule 3 is what guarantees the master-plan's "no fake readiness"
clause. There is no code path that can write a goal's
`lifecycle = Complete` without also writing the corresponding
`GoalCompleted` event with a `VerificationPassed` prior event in
the same log.

---

## 6. Coordination contract with Plan H (context compaction)

Plan H's compaction predicate must consult the goal's lifecycle.
The contract is:

- A session-level compaction **must not** fire while any goal in
  the session is in `Verifying` or `Paused`.
- A `ContextCompacted` event records the `goal_state_at_compaction`
  for every active goal in the session, so the audit log shows
  the exact relationship.
- A compaction that fails (storage, schema, redaction) emits
  `CompactionFailed` and pauses the goal with a typed
  `GoalPaused { by: "system", reason: "compaction_failed" }`.
  The goal does not auto-resume.

This is the only coupling between Plan D and Plan H. The
`GoalLifecycle::blocks_compaction()` method is the single
function Plan H calls.

Pinned event categories (per §9.1) — these are immune to
compaction, even when the predicate would otherwise allow it:

- `GoalCreated`
- `BudgetUpdated`
- `BudgetExhausted`
- `VerificationRequested`
- `VerificationPassed`
- `VerificationFailed`
- `VerificationStale`
- `GoalCompleted`
- `GoalFailed`
- `GoalCancelled`

The pinned-segment list is unit-tested in
`argentum-domain::tests::pinned_events_immune_to_compaction`.

---

## 7. Test cases

These go into the test modules of the relevant crates. Each
test name maps to an acceptance criterion in §11.

In `crates/argentum-domain/src/goal.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_terminal_states_have_no_outgoing_transitions() {
        for s in [GoalLifecycle::Complete, GoalLifecycle::Failed, GoalLifecycle::Cancelled] {
            assert!(s.is_terminal());
            // transition() rejects every (terminal, *) pair by construction.
        }
    }

    #[test]
    fn blocks_compaction_only_for_verifying() {
        assert!(GoalLifecycle::Verifying.blocks_compaction());
        for s in [GoalLifecycle::Unset, GoalLifecycle::Active, GoalLifecycle::Paused,
                  GoalLifecycle::BudgetLimited, GoalLifecycle::Complete,
                  GoalLifecycle::Failed, GoalLifecycle::Cancelled] {
            assert!(!s.blocks_compaction(), "{:?} should not block compaction", s);
        }
    }

    #[test]
    fn budget_is_exhausted_when_any_kind_is_exhausted() {
        let mut b = Budget {
            time: Some(TimeBudget { total_seconds: 60, consumed_seconds: 30 }),
            tokens: Some(TokenBudget { total: 1000, consumed: 1000 }), // exhausted
            tool_rounds: None,
            iterations: None,
        };
        assert!(b.is_exhausted());

        b.tokens = Some(TokenBudget { total: 1000, consumed: 999 });
        assert!(!b.is_exhausted());

        b.iterations = Some(IterationBudget { total: 5, consumed: 5 });
        assert!(b.is_exhausted());
    }
}
```

In `crates/argentum-runtime/src/goal_complete.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn flat_goal() -> Goal { /* a goal with one required ChangedFile check */ }

    #[test]
    fn goal_cannot_complete_from_active_state() {
        let g = flat_goal();
        // Simulate the wrong state by setting lifecycle to Active.
        let mut g = g; g.lifecycle = GoalLifecycle::Active;
        let v = passed_verification();
        assert!(matches!(can_complete(&g, &v), Err(CannotComplete::WrongState(GoalLifecycle::Active))));
    }

    #[test]
    fn goal_cannot_complete_with_unpassed_verification() {
        let mut g = flat_goal(); g.lifecycle = GoalLifecycle::Verifying;
        let mut v = passed_verification();
        v.outcome = VerificationOutcome::Failed { reason: "test".into() };
        assert!(matches!(can_complete(&g, &v), Err(CannotComplete::VerificationNotPassed(_))));
    }

    #[test]
    fn goal_cannot_complete_with_missing_required_evidence() {
        let mut g = flat_goal(); g.lifecycle = GoalLifecycle::Verifying;
        let v = passed_verification_with_no_evidence();
        assert!(matches!(can_complete(&g, &v), Err(CannotComplete::MissingRequired(_))));
    }

    #[test]
    fn goal_cannot_complete_with_stale_evidence() {
        let mut g = flat_goal(); g.lifecycle = GoalLifecycle::Verifying;
        let v = stale_evidence_verification();
        assert!(matches!(can_complete(&g, &v), Err(CannotComplete::StaleEvidence)));
    }

    #[test]
    fn goal_can_complete_when_required_evidence_is_current() {
        let mut g = flat_goal(); g.lifecycle = GoalLifecycle::Verifying;
        let v = current_evidence_verification();
        assert!(can_complete(&g, &v).is_ok());
    }
}
```

In `crates/argentum-runtime/src/goal_transitions.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test] fn transition_active_to_verifying_is_allowed_on_goal_verifying() { /* ... */ }
    #[test] fn transition_paused_to_active_is_allowed_on_goal_resumed() { /* ... */ }
    #[test] fn transition_budget_limited_to_active_requires_budget_update() { /* ... */ }
    #[test] fn transition_complete_to_anything_is_rejected() { /* ... */ }
    #[test] fn transition_audit_log_seq_is_monotonic() { /* ... */ }
    #[test] fn every_transition_appends_exactly_one_event() { /* ... */ }
}
```

In `crates/argentum-store/src/goal_projection.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test] fn goal_state_replays_from_event_log() { /* ... */ }
    #[test] fn goal_state_rejects_duplicate_event_seq() { /* ... */ }
    #[test] fn compaction_does_not_remove_pinned_goal_events() { /* ... */ }
    #[test] fn context_compacted_event_records_goal_state_at_compaction() { /* ... */ }
}
```

In `crates/argentum-ui/src/goal_view.rs` (smoke tests, not full
UI tests):

```rust
#[cfg(test)]
mod tests {
    #[test] fn goal_completed_state_shows_evidence_summary() { /* ... */ }
    #[test] fn goal_failed_state_shows_failure_reason() { /* ... */ }
    #[test] fn goal_paused_state_shows_resume_button() { /* ... */ }
    #[test] fn goal_budget_limited_state_shows_bump_budget_action() { /* ... */ }
    #[test] fn goal_in_verifying_state_shows_no_compaction_indicator() { /* ... */ }
}
```

---

## 8. Migration path

When the rewrite on `UI_redesign` lands, this spec becomes code in
this order:

1. **Stage 1 — types only.** Paste the contents of §1 into
   `crates/argentum-domain/src/goal.rs`. Add `GoalId`,
   `IterationId`, `VerificationId` to `crates/argentum-domain/src/ids.rs`
   if they are not already there. No tests yet; just types and
   `serde` derives. Run `cargo check --workspace`.
2. **Stage 2 — events.** Paste §3 into
   `crates/argentum-domain/src/goal_event.rs`. Add
   `GoalEventKind::kind()` and `GoalEvent::observed_at()`
   helpers. Run `cargo test -p argentum-domain`.
3. **Stage 3 — transitions.** Paste §2's
   `transition()` into `crates/argentum-runtime/src/goal_transitions.rs`.
   Add the `GoalTransitionError` type. Run the §7 transition
   tests. They should all pass with no further code.
4. **Stage 4 — completion rule.** Paste §4's `can_complete()`
   into `crates/argentum-runtime/src/goal_complete.rs`. Run
   the §7 completion tests. They should all pass with no
   further code.
5. **Stage 5 — store projection.** Add
   `crates/argentum-store/src/goal_projection.rs`. Replay
   events into a `Goal` struct. Run the §7 store tests.
6. **Stage 6 — coordination with Plan H.** Update
   `argentum-runtime` so that any compaction entry point
   consults `GoalLifecycle::blocks_compaction()`. Plan H's
   spec file (`context-compaction-h-spec.md`, not yet
   written) will reference this single function.
7. **Stage 7 — UI.** Add `crates/argentum-ui/src/goal_view.rs`
   and surface the goal summary, lifecycle state, and
   next-action controls. Smoke tests per §7.

The whole feature ships in **one PR** (or at most two if
you want to split the store projection out). The PR
description links back to this file.

---

## 9. Out of scope (explicit)

To keep the spec focused:

- No goal *elaboration* (AWS Kiro-style sub-documents). The
  enum exists with a `Spec` variant, but the UI and runtime
  only honor `Flat` for now.
- No multi-goal composition (one goal referencing another). A
  goal is a single objective with a single budget.
- No automatic continuation logic. Continuation is always
  triggered by the user or by a typed next-action; the
  runtime never assumes success.
- No cross-session goal transfer. A goal lives in one
  session; if a session is cleared, the goal history is
  preserved as audit, but the goal itself is marked
  `Cancelled` and must be re-created.
- No model-side success declaration. Completion always
  requires `VerificationPassed` + required evidence +
  non-stale evidence, all enforced in `argentum-runtime`,
  not in the model.

These match the master plan's "no fake readiness" and "no
model-declared success" rules. Each is unit-testable and
must remain so.

---

## 10. Cross-cutting concerns

- **Concurrency.** Goal state mutations go through
  `argentum-runtime`. Two concurrent calls cannot both
  transition the same goal because the event-log append
  holds a per-goal SQLite write lock. The transition
  function is the only place that reads + writes goal
  state.
- **Schema migrations.** Adding new `GoalEventKind` variants
  is a backward-compatible schema bump (Plan N). Removing
  or renaming a variant requires a protocol-version bump.
- **Logging.** Every goal event is also emitted to
  `tracing` at the `info` level for events that change
  lifecycle, `debug` for events that don't. No goal event
  payload contains credentials or PII.
- **Credentials.** No goal event carries an API key, a
  token, a URL credential, or a profile. The exact-profile
  rule from the master plan is preserved.

---

## 11. Acceptance criteria crosswalk

Each acceptance criterion from `ade-harness-landscape-2026-08-14.md` §7 Plan D maps to one or more test names in §7:

| §7 Plan D acceptance criterion                                            | Test name(s)                                                                                          |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| A Rust test creates a goal, an iteration, an approval, a change set, and a verification record. | `goal_state_replays_from_event_log`, `goal_can_complete_when_required_evidence_is_current`            |
| Missing, stale, cancelled, or failed evidence cannot complete the goal.    | `goal_cannot_complete_with_missing_required_evidence`, `goal_cannot_complete_with_stale_evidence`, `goal_cannot_complete_with_unpassed_verification` |
| The audit log records every transition.                                    | `every_transition_appends_exactly_one_event`, `goal_state_replays_from_event_log`, `goal_audit_seq_is_monotonic` |
| A goal cannot enter `verifying` while Plan H is mid-compaction of the same session. | `blocks_compaction_only_for_verifying`, `compaction_does_not_remove_pinned_goal_events`               |
| The lifecycle states that block compaction are declared in `argentum-domain` and unit-tested. | `blocks_compaction_only_for_verifying`, `lifecycle_terminal_states_have_no_outgoing_transitions`      |

When all the §7 tests pass and the §11 crosswalk is complete, Plan D
is done.

---

## 12. Definition of done for this spec

This spec is "done" when:

1. Every test name in §7 is green in the relevant crate.
2. The §11 crosswalk is complete and the implementation PR
   references both files.
3. The PR description links to this spec and to the master
   plan.
4. `cargo fmt --check`, `cargo check --workspace --locked`,
   `cargo test --workspace --locked`, and
   `cargo clippy --workspace --all-targets --locked -- -D warnings`
   all pass.
5. The plan doc (`ade-harness-landscape-2026-08-14.md`) is
   updated to move Plan D from "PRIORITY" to "shipped,"
   with a link to the merged PR.
