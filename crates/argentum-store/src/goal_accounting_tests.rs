use super::*;
use argentum_domain::context_budget::{ModelBudget, RequestAccounting};

fn fixture() -> (tempfile::TempDir, Store, Goal, RunId) {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let store = Store::open(directory.path().join("accounting.db")).unwrap();
    let snapshot = store.resolve_workspace(&workspace).unwrap().snapshot;
    let goal = Goal {
        id: Uuid::new_v4(),
        project_id: snapshot.project.id,
        session_id: snapshot.active_session_id.unwrap(),
        objective: "Synthetic accounting boundary".into(),
        lifecycle: GoalLifecycle::Active,
        token_budget: Some(1000),
        tool_budget: Some(2),
        time_budget_seconds: Some(60),
        tokens_used: 0,
        tools_used: 0,
        iteration: 0,
        next_action: "Explicit work only".into(),
        verification_history: Vec::new(),
        created_at: now(),
        updated_at: now(),
        accounting: Some(GoalAccounting::new(false)),
        definition_revision: Uuid::new_v4(),
        acceptance: None,
        owner_attestation: None,
        completion: None,
    };
    store.save_goal(&goal).unwrap();
    let run = Uuid::new_v4();
    store
        .admit_goal_run(
            goal.project_id,
            goal.session_id,
            GoalBinding {
                goal_id: goal.id,
                revision: goal.accounting.as_ref().unwrap().revision,
            },
            run,
            false,
        )
        .unwrap();
    (directory, store, goal, run)
}

fn estimate() -> RequestAccounting {
    RequestAccounting::from_wire_bytes(
        &ModelBudget {
            revision: Uuid::new_v4(),
            profile_id: "synthetic".into(),
            provider_kind: argentum_domain::ProviderKind::OpenAiCompatible,
            endpoint: "http://127.0.0.1:1/v1/".into(),
            model: "synthetic".into(),
            usable_context_tokens: 10000,
            input_limit_tokens: None,
            maximum_output_tokens: 100,
            requested_output_tokens: 100,
            uncertainty_tokens: 128,
        },
        100,
        10,
        10,
    )
    .unwrap()
}

fn usage(total: u64) -> ObservedUsage {
    ObservedUsage {
        input_tokens: None,
        output_tokens: None,
        total_tokens: total,
        cached_input_tokens: None,
        reasoning_tokens: None,
    }
}

#[test]
fn aggregate_goal_limit_increase_preserves_charges_and_run_limits_on_reload() {
    let (directory, store, goal, run) = fixture();
    let request = Uuid::new_v4();
    store
        .reserve_goal_request(run, request, "main_generation", &estimate())
        .unwrap();
    store.goal_request_dispatched(request).unwrap();
    store.settle_goal_request(request, Some(usage(20))).unwrap();
    store
        .finish_goal_run(run, 19, "task_finished_goal_not_verified")
        .unwrap();
    let limits = [10_000_000, 256, 128, 768, 7200, 10_000_000, 32, 256, 295];
    let before = store
        .configure_goal_limits(goal.project_id, goal.session_id, goal.id, limits)
        .unwrap();
    let mut raised = limits;
    raised[0] = 20_000_000;
    let after = store
        .configure_goal_limits(goal.project_id, goal.session_id, goal.id, raised)
        .unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.tokens_used, before.tokens_used);
    assert_eq!(after.tools_used, before.tools_used);
    assert_eq!(after.iteration, before.iteration);
    assert_eq!(after.definition_revision, before.definition_revision);
    let old = before.accounting.as_ref().unwrap();
    let new = after.accounting.as_ref().unwrap();
    assert_eq!(new.counters, old.counters);
    assert_eq!(new.per_run, old.per_run);
    assert_eq!(new.request_limit, old.request_limit);
    assert_eq!(new.iteration_limit, old.iteration_limit);
    assert_ne!(new.revision, old.revision);
    drop(store);
    let reopened = Store::open(directory.path().join("accounting.db")).unwrap();
    assert_eq!(
        reopened
            .goal(goal.project_id, goal.session_id)
            .unwrap()
            .unwrap(),
        after
    );
    let inspection = reopened
        .inspect_goal_accounting(goal.project_id, goal.session_id, goal.id)
        .unwrap();
    assert_eq!(inspection.requests.len(), 1);
    assert_eq!(inspection.requests[0].request_id, request);
    assert_eq!(
        inspection.requests[0]
            .observed
            .as_ref()
            .unwrap()
            .total_tokens,
        20
    );
    assert_eq!(after.token_budget, Some(20_000_000));
    assert_eq!(new.counters.observed_tokens, 20);
}

#[test]
fn aggregate_goal_limit_increase_retains_invalid_scope_overflow_and_run_caps() {
    let (_directory, store, goal, _run) = fixture();
    let limits = [20_000_000, 256, 128, 768, 7200, 10_000_000, 32, 256, 295];
    let before = store
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .unwrap();
    for invalid in [0, 20_000_001, u64::MAX] {
        let mut rejected = limits;
        rejected[0] = invalid;
        assert!(store
            .configure_goal_limits(goal.project_id, goal.session_id, goal.id, rejected)
            .is_err());
    }
    let mut rejected = limits;
    rejected[5] = 10_000_001;
    assert!(store
        .configure_goal_limits(goal.project_id, goal.session_id, goal.id, rejected)
        .is_err());
    rejected = limits;
    rejected[1] = u64::MAX;
    assert!(store
        .configure_goal_limits(goal.project_id, goal.session_id, goal.id, rejected)
        .is_err());
    assert!(store
        .configure_goal_limits(Uuid::new_v4(), goal.session_id, goal.id, limits)
        .is_err());
    assert!(store
        .configure_goal_limits(goal.project_id, Uuid::new_v4(), goal.id, limits)
        .is_err());
    assert_eq!(
        store
            .goal(goal.project_id, goal.session_id)
            .unwrap()
            .unwrap(),
        before
    );
}

#[test]
fn explicit_remove_aggregate_preserves_charges_scope_and_finite_runs_after_reload() {
    let (directory, store, goal, run) = fixture();
    let configured = store
        .configure_goal_limits(
            goal.project_id,
            goal.session_id,
            goal.id,
            [1000, 1, 1, 2, 60, 1000, 1, 2, 60],
        )
        .unwrap();
    let request = Uuid::new_v4();
    store
        .reserve_goal_request(run, request, "main_generation", &estimate())
        .unwrap();
    store.goal_request_dispatched(request).unwrap();
    store.settle_goal_request(request, Some(usage(20))).unwrap();
    store
        .finish_goal_run(run, 19, "goal_request_limit")
        .unwrap();
    let before = store
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .unwrap();
    assert!(store
        .remove_goal_aggregate_limits(Uuid::new_v4(), goal.session_id, goal.id)
        .is_err());
    assert!(store
        .remove_goal_aggregate_limits(goal.project_id, Uuid::new_v4(), goal.id)
        .is_err());
    assert_eq!(
        store
            .goal(goal.project_id, goal.session_id)
            .unwrap()
            .unwrap(),
        before
    );
    let after = store
        .remove_goal_aggregate_limits(goal.project_id, goal.session_id, goal.id)
        .unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.definition_revision, before.definition_revision);
    assert_eq!(after.tokens_used, 20);
    assert!(
        after.token_budget.is_none()
            && after.tool_budget.is_none()
            && after.time_budget_seconds.is_none()
    );
    let account = after.accounting.as_ref().unwrap();
    assert!(account.owner_removed_aggregate_limits);
    assert_eq!(
        account.counters,
        before.accounting.as_ref().unwrap().counters
    );
    assert_eq!(account.per_run, configured.accounting.unwrap().per_run);
    assert!(account.request_limit.is_none() && account.iteration_limit.is_none());
    drop(store);
    let reopened = Store::open(directory.path().join("accounting.db")).unwrap();
    assert_eq!(
        reopened
            .goal(goal.project_id, goal.session_id)
            .unwrap()
            .unwrap(),
        after
    );
    let binding = GoalBinding {
        goal_id: after.id,
        revision: account.revision,
    };
    let next_run = Uuid::new_v4();
    reopened
        .admit_goal_run(goal.project_id, goal.session_id, binding, next_run, false)
        .unwrap();
    let next_request = Uuid::new_v4();
    reopened
        .reserve_goal_request(next_run, next_request, "main_generation", &estimate())
        .unwrap();
    reopened.goal_request_dispatched(next_request).unwrap();
    reopened
        .settle_goal_request(next_request, Some(usage(20)))
        .unwrap();
    assert!(matches!(
        reopened.reserve_goal_request(next_run, Uuid::new_v4(), "main_generation", &estimate()),
        Err(StoreError::GoalAccounting("goal_request_limit"))
    ));
}

#[test]
fn removing_aggregate_cannot_clear_unknown_usage_or_missing_run_guards() {
    let (_directory, store, goal, run) = fixture();
    assert!(matches!(
        store.remove_goal_aggregate_limits(goal.project_id, goal.session_id, goal.id),
        Err(StoreError::GoalAccounting("finite_per_run_limits_required"))
    ));
    store
        .configure_goal_limits(
            goal.project_id,
            goal.session_id,
            goal.id,
            [1000, 2, 2, 2, 60, 1000, 2, 2, 60],
        )
        .unwrap();
    let request = Uuid::new_v4();
    store
        .reserve_goal_request(run, request, "main_generation", &estimate())
        .unwrap();
    store.goal_request_dispatched(request).unwrap();
    store.settle_goal_request(request, None).unwrap();
    store
        .finish_goal_run(run, 20, "goal_usage_unresolved")
        .unwrap();
    let before = store
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .unwrap();
    let after = store
        .remove_goal_aggregate_limits(goal.project_id, goal.session_id, goal.id)
        .unwrap();
    assert_eq!(
        after.accounting.as_ref().unwrap().counters,
        before.accounting.unwrap().counters
    );
    assert!(matches!(
        store.admit_goal_run(
            goal.project_id,
            goal.session_id,
            GoalBinding {
                goal_id: goal.id,
                revision: after.accounting.unwrap().revision
            },
            Uuid::new_v4(),
            false
        ),
        Err(StoreError::GoalAccounting("goal_usage_unresolved"))
    ));
}

#[test]
fn f07a_finite_request_iteration_and_tool_admissions_are_distinct() {
    let (_directory, store, goal, run) = fixture();
    let configured = store
        .configure_goal_limits(
            goal.project_id,
            goal.session_id,
            goal.id,
            [1000, 1, 1, 1, 60, 1000, 1, 1, 60],
        )
        .unwrap();
    let request = Uuid::new_v4();
    store
        .reserve_goal_request(run, request, "compaction_tools_disabled", &estimate())
        .unwrap();
    store.goal_request_dispatched(request).unwrap();
    store.settle_goal_request(request, Some(usage(20))).unwrap();
    assert!(matches!(
        store.reserve_goal_request(run, Uuid::new_v4(), "main_generation", &estimate()),
        Err(StoreError::GoalAccounting("goal_request_limit"))
    ));
    store.admit_goal_tool(run).unwrap();
    store.release_uninvoked_goal_tool(run).unwrap();
    assert_eq!(
        store
            .goal_run_accounting(run)
            .unwrap()
            .unwrap()
            .counters
            .tool_admissions,
        0
    );
    store.admit_goal_tool(run).unwrap();
    assert!(matches!(
        store.admit_goal_tool(run),
        Err(StoreError::GoalAccounting("goal_tool_limit"))
    ));
    store.finish_goal_run(run, 10, "goal_tool_limit").unwrap();
    let binding = GoalBinding {
        goal_id: goal.id,
        revision: configured.accounting.unwrap().revision,
    };
    assert!(matches!(
        store.admit_goal_run(
            goal.project_id,
            goal.session_id,
            binding,
            Uuid::new_v4(),
            false
        ),
        Err(StoreError::GoalAccounting("goal_iteration_limit"))
    ));
    let stored = store
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.iteration, 1);
    assert_eq!(stored.tools_used, 1);
    assert_eq!(stored.tokens_used, 20);
}

#[test]
fn f07a_checkpoint_exclusion_is_limited_to_successful_accounting_writes() {
    let (_directory, store, goal, run) = fixture();
    let before = store
        .summary_snapshot(goal.project_id, goal.session_id)
        .unwrap()
        .token;
    let request = Uuid::new_v4();
    store
        .reserve_goal_request(run, request, "compaction_tools_disabled", &estimate())
        .unwrap();
    store.goal_request_dispatched(request).unwrap();
    store.settle_goal_request(request, Some(usage(20))).unwrap();
    assert_eq!(
        store
            .summary_snapshot(goal.project_id, goal.session_id)
            .unwrap()
            .token,
        before
    );
    store
        .configure_goal_limits(
            goal.project_id,
            goal.session_id,
            goal.id,
            [1000, 10, 10, 10, 60, 1000, 10, 10, 60],
        )
        .unwrap();
    let owner_edit = store
        .summary_snapshot(goal.project_id, goal.session_id)
        .unwrap()
        .token;
    assert_ne!(owner_edit, before);
    store.connection.lock().unwrap().execute_batch("CREATE TRIGGER extra_accounting_write AFTER INSERT ON goal_requests BEGIN UPDATE sessions SET payload = payload; END;").unwrap();
    store
        .reserve_goal_request(run, Uuid::new_v4(), "main_generation", &estimate())
        .unwrap();
    assert_ne!(
        store
            .summary_snapshot(goal.project_id, goal.session_id)
            .unwrap()
            .token,
        owner_edit
    );
}

#[test]
fn f07a_legacy_history_is_marked_incomplete_not_reconstructed_or_reset() {
    let (directory, store, goal, run) = fixture();
    store
        .finish_goal_run(run, 1, "task_finished_goal_not_verified")
        .unwrap();
    store.clear_goal(goal.project_id, goal.session_id).unwrap();
    let mut legacy = goal.clone();
    legacy.id = Uuid::new_v4();
    legacy.accounting = None;
    legacy.tokens_used = 73;
    legacy.iteration = 2;
    store.save_goal(&legacy).unwrap();
    drop(store);
    let reopened = Store::open(directory.path().join("accounting.db")).unwrap();
    let migrated = reopened
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .unwrap();
    assert_eq!(migrated.id, legacy.id);
    assert_eq!(migrated.tokens_used, 73);
    assert_eq!(migrated.iteration, 2);
    let accounting = migrated.accounting.as_ref().unwrap();
    assert!(accounting.incomplete_history);
    assert!(accounting
        .remaining(migrated.token_budget)
        .estimated_tokens
        .is_none());
    let binding = GoalBinding {
        goal_id: migrated.id,
        revision: accounting.revision,
    };
    assert!(matches!(
        reopened.admit_goal_run(
            goal.project_id,
            goal.session_id,
            binding,
            Uuid::new_v4(),
            false
        ),
        Err(StoreError::GoalAccounting(
            "legacy_usage_history_unavailable"
        ))
    ));
    // Increasing the old declaration does not transform unknown history into
    // a freshly funded goal. The separate identity is still required.
    reopened
        .configure_goal_limits(
            goal.project_id,
            goal.session_id,
            migrated.id,
            [5000, 10, 10, 10, 60, 1000, 10, 10, 60],
        )
        .unwrap();
    let after = reopened
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .unwrap();
    assert!(after.accounting.unwrap().incomplete_history);
    assert_eq!(after.tokens_used, 73);
}

#[test]
fn f07a_duplicate_late_settlement_preserves_identity_and_full_overrun_after_clear() {
    let (_directory, store, goal, run) = fixture();
    let request = Uuid::new_v4();
    store
        .reserve_goal_request(run, request, "main_generation", &estimate())
        .unwrap();
    store.goal_request_dispatched(request).unwrap();
    store
        .configure_goal_limits(
            goal.project_id,
            goal.session_id,
            goal.id,
            [50, 10, 10, 10, 60, 1000, 10, 10, 60],
        )
        .unwrap();
    store.clear_goal(goal.project_id, goal.session_id).unwrap();
    let observed = usage(1200);
    let first = store
        .settle_goal_request(request, Some(observed.clone()))
        .unwrap();
    assert_eq!(first.binding.goal_id, goal.id);
    assert_eq!(first.observed.as_ref().unwrap().total_tokens, 1200);
    assert!(first.overrun_tokens >= 1150);
    assert_eq!(
        store.settle_goal_request(request, Some(observed)).unwrap(),
        first
    );
    assert!(matches!(
        store.settle_goal_request(request, Some(usage(1199))),
        Err(StoreError::GoalAccounting("goal_settlement_conflict"))
    ));
    let ended = store
        .finish_goal_run(run, 19, "goal_usage_overrun")
        .unwrap();
    assert_eq!(ended.counters.observed_tokens, 1200);
    assert_eq!(ended.counters.elapsed_milliseconds, 19);
    assert_eq!(store.finish_goal_run(run, 500, "failed").unwrap(), ended);
    assert!(store
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .is_none());
}

#[test]
fn f07a_parallel_reservations_cannot_double_spend_and_restart_does_not_refund() {
    let (directory, store, goal, run) = fixture();
    let other = Store::open(directory.path().join("accounting.db")).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            barrier.wait();
            other.reserve_goal_request(run, Uuid::new_v4(), "main_generation", &estimate())
        });
        barrier.wait();
        let result =
            store.reserve_goal_request(run, Uuid::new_v4(), "main_generation", &estimate());
        [result, worker.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let charge = results.into_iter().find_map(Result::ok).unwrap();
    store.goal_request_dispatched(charge.request_id).unwrap();
    store.settle_goal_request(charge.request_id, None).unwrap();
    store
        .finish_goal_run(run, 20, "goal_usage_unresolved")
        .unwrap();
    let before = store
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .unwrap();
    drop(other);
    drop(store);
    let reopened = Store::open(directory.path().join("accounting.db")).unwrap();
    assert_eq!(
        reopened
            .goal(goal.project_id, goal.session_id)
            .unwrap()
            .unwrap(),
        before
    );
    assert!(matches!(
        reopened.admit_goal_run(
            goal.project_id,
            goal.session_id,
            charge.binding,
            Uuid::new_v4(),
            false
        ),
        Err(StoreError::GoalAccounting("goal_usage_unresolved"))
    ));
    assert_eq!(
        before.accounting.unwrap().counters.unresolved_tokens,
        charge.reserved_tokens
    );
}

#[test]
fn f07a_known_nondispatch_and_overflow_are_not_usage_or_free_dispatched_refunds() {
    let (_directory, store, goal, run) = fixture();
    let mut overflow = estimate();
    overflow.accounted_input = u64::MAX;
    assert!(matches!(
        store.reserve_goal_request(run, Uuid::new_v4(), "main_generation", &overflow),
        Err(StoreError::GoalAccounting("accounting_overflow"))
    ));
    let request = Uuid::new_v4();
    store
        .reserve_goal_request(run, request, "main_generation", &estimate())
        .unwrap();
    store.goal_request_not_dispatched(request).unwrap();
    store.goal_request_not_dispatched(request).unwrap();
    let next = Uuid::new_v4();
    store
        .reserve_goal_request(run, next, "compaction_tools_disabled", &estimate())
        .unwrap();
    store.goal_request_dispatched(next).unwrap();
    assert!(store.goal_request_not_dispatched(next).is_err());
    store.settle_goal_request(next, Some(usage(0))).unwrap();
    let current = store
        .goal(goal.project_id, goal.session_id)
        .unwrap()
        .unwrap();
    let counters = current.accounting.unwrap().counters;
    assert_eq!(counters.request_attempts, 2);
    assert_eq!(counters.observed_tokens, 0);
    assert_eq!(counters.outstanding_tokens, 0);
    assert_eq!(counters.unresolved_tokens, 0);
}
