use super::*;
use argentum_domain::{
    context_budget::{ModelBudget, RequestAccounting},
    recovery::RecoveryRunKind,
    GoalLifecycle,
};

struct Fixture {
    root: tempfile::TempDir,
    db: PathBuf,
    workspace: PathBuf,
    goal: Goal,
    request: Uuid,
    reservation: u64,
}

fn provider_only_interruption(f: &Fixture) -> Uuid {
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    let mut record = f.inspect().ownership;
    let revision = Uuid::new_v4();
    record.execution_state = RecoveryExecutionState::Interrupted;
    record.recovered_at = Some(now());
    record.recovery_revision = Some(revision);
    record.owner_reviewed_at = None;
    record.authority_invalidated = true;
    record.admission_blocked = true;
    record.effect_state = RecoveryEffectState::None;
    record.known_effects.clear();
    record.unresolved_effects = vec!["provider_request_outcome_unknown".into()];
    store
        .connection
        .lock()
        .unwrap()
        .execute(
            "UPDATE runtime_run_ownership SET state='interrupted',payload=?2 WHERE run_id=?1",
            params![
                record.run_id.to_string(),
                serde_json::to_string(&record).unwrap()
            ],
        )
        .unwrap();
    owner.retire().unwrap();
    revision
}

#[test]
fn interrupted_provider_full_retention_requires_exact_review_and_survives_restart() {
    let f = Fixture::new();
    let revision = provider_only_interruption(&f);
    let before = f.inspect();
    assert!(!before.recoverable);
    assert!(Store::review_recovery(&f.db, before.binding.run_id, Uuid::new_v4()).is_err());
    Store::review_recovery(&f.db, before.binding.run_id, revision).unwrap();
    assert!(f.inspect().recoverable);
    let decision = f.decision();
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    let result = store.resolve_accounting_recovery(&decision).unwrap();
    assert_eq!(
        result.disposition.as_ref().unwrap().client_charge_tokens,
        f.reservation
    );
    assert!(result.observed.is_none());
    assert_eq!(result.state, ChargeState::Unresolved);
    owner.retire().unwrap();
    drop(store);
    let after = f.inspect();
    assert!(after.ownership.unresolved_effects.is_empty());
    assert_eq!(
        after.ownership.execution_state,
        RecoveryExecutionState::Interrupted
    );
    assert_eq!(
        after.ownership.request_state,
        before.ownership.request_state
    );
    assert!(after.ownership.authority_invalidated);
    assert_eq!(after.ownership.recovery_revision, Some(revision));
    assert!(after.charge.observed.is_none());
    assert_eq!(after.goal_counters.owner_accepted_tokens, f.reservation);
    assert_eq!(after.goal_counters.unresolved_tokens, 0);
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    assert_eq!(
        store.resolve_accounting_recovery(&decision).unwrap(),
        result
    );
    owner.retire().unwrap();
}

#[test]
fn interrupted_provider_retention_never_waives_file_process_or_other_run_uncertainty() {
    for reason in [
        "uncontained_process_may_still_be_running",
        "workspace_change_outcome_unknown",
        "approval_claimed_without_settled_effect",
    ] {
        let f = Fixture::new();
        let revision = provider_only_interruption(&f);
        let record = f.inspect().ownership;
        Store::review_recovery(&f.db, record.run_id, revision).unwrap();
        let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
        store.connection.lock().unwrap().execute(
            "UPDATE runtime_run_ownership SET payload=json_set(payload,'$.unresolved_effects',json(?2)) WHERE run_id=?1",
            params![record.run_id.to_string(), serde_json::to_string(&vec!["provider_request_outcome_unknown", reason]).unwrap()],
        ).unwrap();
        assert!(!f.inspect().recoverable);
        assert!(store.resolve_accounting_recovery(&f.decision()).is_err());
        owner.retire().unwrap();
    }
}

#[test]
#[ignore = "requires exact packaged CLI supplied through ARGENTUM_F01_CLI_EXE"]
fn interrupted_provider_exact_cli_review_and_retention() {
    let executable = std::env::var_os("ARGENTUM_F01_CLI_EXE").unwrap();
    let f = Fixture::new();
    let revision = provider_only_interruption(&f);
    let before = f.inspect();
    let call = |args: &[String]| {
        let mut command = std::process::Command::new(&executable);
        command.args(args);
        if args[0] != "recovery" {
            command.args(["--workspace", f.workspace.to_str().unwrap()]);
        }
        command.args(["--database", f.db.to_str().unwrap(), "--json"]);
        command.output().unwrap()
    };
    let reviewed = call(&[
        "recovery".into(),
        "review".into(),
        "--run-id".into(),
        before.binding.run_id.to_string(),
        "--revision".into(),
        revision.to_string(),
    ]);
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    let decision = f.decision();
    let file = f.root.path().join("exact-resolution.json");
    std::fs::write(&file, serde_json::to_vec(&decision).unwrap()).unwrap();
    let resolved = call(&[
        "accounting".into(),
        "resolve".into(),
        "--resolution".into(),
        file.to_str().unwrap().into(),
        "--confirm-reservation".into(),
        f.reservation.to_string(),
    ]);
    assert!(
        resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let inspected = call(&[
        "accounting".into(),
        "inspect".into(),
        "--session-id".into(),
        f.goal.session_id.to_string(),
        "--goal-id".into(),
        f.goal.id.to_string(),
        "--request-id".into(),
        f.request.to_string(),
    ]);
    assert!(inspected.status.success());
    let after: AccountingRecoveryInspection = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(
        after.charge.disposition.unwrap().client_charge_tokens,
        f.reservation
    );
    assert!(after.charge.observed.is_none());
    assert!(after.ownership.unresolved_effects.is_empty());
    assert_eq!(
        after.ownership.execution_state,
        RecoveryExecutionState::Interrupted
    );
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let db = root.path().join("state.db");
        let (store, mut owner) = Store::open_for_host(&db).unwrap();
        let snapshot = store.resolve_workspace(&workspace).unwrap().snapshot;
        let goal = Goal {
            id: Uuid::new_v4(),
            project_id: snapshot.project.id,
            session_id: snapshot.active_session_id.unwrap(),
            objective: "Synthetic owner accounting recovery".into(),
            lifecycle: GoalLifecycle::Active,
            token_budget: Some(1_000_000),
            tool_budget: Some(32),
            time_budget_seconds: Some(60),
            tokens_used: 0,
            tools_used: 0,
            iteration: 0,
            next_action: "explicit work only".into(),
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
            .claim_run_ownership(
                run,
                goal.project_id,
                goal.session_id,
                None,
                None,
                RecoveryRunKind::Compaction,
            )
            .unwrap();
        store
            .admit_goal_run(
                goal.project_id,
                goal.session_id,
                GoalBinding {
                    goal_id: goal.id,
                    revision: goal.accounting.as_ref().unwrap().revision,
                },
                run,
                true,
            )
            .unwrap();
        let estimate = RequestAccounting::from_wire_bytes(
            &ModelBudget {
                revision: Uuid::new_v4(),
                profile_id: "synthetic".into(),
                provider_kind: argentum_domain::ProviderKind::OpenAiCompatible,
                endpoint: "http://127.0.0.1:1/".into(),
                model: "synthetic".into(),
                usable_context_tokens: 204800,
                input_limit_tokens: Some(196608),
                maximum_output_tokens: 4096,
                requested_output_tokens: 4096,
                uncertainty_tokens: 1024,
            },
            149807,
            0,
            0,
        )
        .unwrap();
        let request = Uuid::new_v4();
        let charge = store
            .reserve_goal_request(run, request, "compaction_tools_disabled", &estimate)
            .unwrap();
        assert_eq!(charge.reserved_tokens, 154927);
        store
            .mark_run_request(
                run,
                request,
                "compaction_tools_disabled",
                argentum_domain::recovery::RecoveryRequestState::PreparedNotDispatched,
            )
            .unwrap();
        store.goal_request_dispatched(request).unwrap();
        store.settle_goal_request(request, None).unwrap();
        store
            .mark_run_request(
                run,
                request,
                "compaction_tools_disabled",
                argentum_domain::recovery::RecoveryRequestState::Dispatched,
            )
            .unwrap();
        store
            .finish_goal_run(run, 100, "goal_usage_unresolved")
            .unwrap();
        store
            .finish_run_ownership(run, RecoveryExecutionState::Failed, "failed")
            .unwrap();
        owner.retire().unwrap();
        Self {
            root,
            db,
            workspace,
            goal,
            request,
            reservation: charge.reserved_tokens,
        }
    }
    fn inspect(&self) -> AccountingRecoveryInspection {
        Store::inspect_accounting_recovery(
            &self.db,
            &self.workspace,
            self.goal.session_id,
            self.goal.id,
            self.request,
        )
        .unwrap()
    }
    fn decision(&self) -> AccountingResolution {
        AccountingResolution {
            command_id: "owner-exact-recovery".into(),
            binding: self.inspect().binding,
            decision: AccountingDecision::OwnerAcceptReservation,
        }
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
}

#[test]
fn owner_recovery_restart_replay_and_same_goal_admission_preserve_missing_usage() {
    let f = Fixture::new();
    let decision = f.decision();
    let before = f.inspect();
    assert!(before.recoverable);
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    let result = store.resolve_accounting_recovery(&decision).unwrap();
    assert_eq!(result.state, ChargeState::Unresolved);
    assert_eq!(result.usage_quality, "missing_final_usage");
    assert!(result.observed.is_none());
    assert_eq!(
        store.resolve_accounting_recovery(&decision).unwrap(),
        result
    );
    let current = store
        .goal(f.goal.project_id, f.goal.session_id)
        .unwrap()
        .unwrap();
    let account = current.accounting.as_ref().unwrap();
    assert_eq!(account.counters.observed_tokens, 0);
    assert_eq!(account.counters.unresolved_tokens, 0);
    assert_eq!(account.counters.effective_tokens(), f.reservation);
    assert_eq!(account.counters.request_attempts, 1);
    assert_ne!(account.revision, decision.binding.accounting_revision);
    assert!(store
        .admit_goal_run(
            f.goal.project_id,
            f.goal.session_id,
            GoalBinding {
                goal_id: f.goal.id,
                revision: account.revision
            },
            Uuid::new_v4(),
            false
        )
        .is_ok());
    // Finish the newly admitted empty interval before closing this owner.
    let run = store
        .inspect_goal_accounting(f.goal.project_id, f.goal.session_id, f.goal.id)
        .unwrap()
        .recent_runs[0]
        .run_id;
    store
        .finish_goal_run(run, 0, "task_finished_goal_not_verified")
        .unwrap();
    owner.retire().unwrap();
    drop(store);
    let (fresh, mut owner) = Store::open_for_host(&f.db).unwrap();
    assert_eq!(
        fresh.resolve_accounting_recovery(&decision).unwrap(),
        result
    );
    owner.retire().unwrap();
    let reloaded = f.inspect();
    assert_eq!(reloaded.charge, result);
    assert_eq!(reloaded.ownership, before.ownership);
    assert_eq!(reloaded.goal_counters.effective_tokens(), f.reservation);
    assert_eq!(f.root.path(), f.db.parent().unwrap());
}

#[test]
fn owner_recovery_rejects_wrong_store_scope_amount_revision_owner_and_conflict() {
    let f = Fixture::new();
    let original = f.decision();
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    for field in 0..11 {
        let mut wrong = original.clone();
        match field {
            0 => wrong.binding.store_id = Uuid::new_v4(),
            1 => wrong.binding.physical_store_sha256.0 = "a".repeat(64),
            2 => wrong.binding.project_id = Uuid::new_v4(),
            3 => wrong.binding.session_id = Uuid::new_v4(),
            4 => wrong.binding.goal_id = Uuid::new_v4(),
            5 => wrong.binding.request_id = Uuid::new_v4(),
            6 => wrong.binding.run_id = Uuid::new_v4(),
            7 => wrong.binding.accounting_revision = Uuid::new_v4(),
            8 => wrong.binding.owner_id = Uuid::new_v4(),
            9 => wrong.binding.owner_generation += 1,
            _ => wrong.binding.reserved_tokens -= 1,
        }
        assert!(
            store.resolve_accounting_recovery(&wrong).is_err(),
            "accepted wrong field {field}"
        );
    }
    assert_eq!(f.inspect().goal_counters.unresolved_tokens, f.reservation);
    store.resolve_accounting_recovery(&original).unwrap();
    let mut conflict = original.clone();
    conflict.decision = AccountingDecision::AuthoritativeUsage {
        event_id: 1,
        usage: Fixture::usage(1),
    };
    assert!(store.resolve_accounting_recovery(&conflict).is_err());
    let mut stale = original;
    stale.command_id = "different-owner-command".into();
    assert!(store.resolve_accounting_recovery(&stale).is_err());
    owner.retire().unwrap();
}

#[test]
fn owner_recovery_transaction_failure_and_concurrent_equal_delivery_charge_once() {
    let f = Fixture::new();
    let decision = f.decision();
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    let before = f.inspect();
    store.connection.lock().unwrap().execute_batch("CREATE TRIGGER reject_recovery_identity BEFORE INSERT ON command_identities BEGIN SELECT RAISE(FAIL,'synthetic transaction failure'); END;").unwrap();
    assert!(store.resolve_accounting_recovery(&decision).is_err());
    assert_eq!(f.inspect(), before);
    store
        .connection
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_recovery_identity;")
        .unwrap();
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..8 {
            let store = store.clone();
            let input = decision.clone();
            workers.push(scope.spawn(move || store.resolve_accounting_recovery(&input)));
        }
        for worker in workers {
            assert!(worker.join().unwrap().is_ok());
        }
    });
    assert_eq!(f.inspect().goal_counters.effective_tokens(), f.reservation);
    assert_eq!(
        store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM command_identities WHERE command_kind='accounting_recovery'",
                [],
                |row| row.get::<_, u32>(0)
            )
            .unwrap(),
        1
    );
    owner.retire().unwrap();
}

#[test]
fn owner_recovery_live_or_uncertain_effects_cannot_be_resolved() {
    let f = Fixture::new();
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    assert!(matches!(
        Store::open_for_host(&f.db),
        Err(StoreError::HostOwnershipLive)
    ));
    for (state, effects) in [("active", "[]"), ("failed", "[\"unknown_process_effect\"]")] {
        let db = store.connection.lock().unwrap();
        db.execute("UPDATE runtime_run_ownership SET state=?2,payload=json_set(payload,'$.execution_state',?2,'$.unresolved_effects',json(?3)) WHERE run_id=?1",params![f.inspect().binding.run_id.to_string(),state,effects]).unwrap();
        drop(db);
        let inspection = f.inspect();
        assert!(!inspection.recoverable);
        let resolution = AccountingResolution {
            command_id: format!("deny-{state}"),
            binding: inspection.binding,
            decision: AccountingDecision::OwnerAcceptReservation,
        };
        assert!(store.resolve_accounting_recovery(&resolution).is_err());
    }
    let generation = owner.generation().unwrap();
    store
        .connection
        .lock()
        .unwrap()
        .execute(
            "UPDATE runtime_host_generations SET state='recovery_pending' WHERE generation=?1",
            [generation],
        )
        .unwrap();
    assert!(store.resolve_accounting_recovery(&f.decision()).is_err());
    store
        .connection
        .lock()
        .unwrap()
        .execute(
            "UPDATE runtime_host_generations SET state='active' WHERE generation=?1",
            [generation],
        )
        .unwrap();
    owner.retire().unwrap();
}

#[test]
fn owner_recovery_preserves_unrelated_session_holds_but_never_clears_its_own_hold() {
    let f = Fixture::new();
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    let other = store
        .create_session(f.goal.project_id, "Unrelated retained session")
        .unwrap();
    let mut record = f.inspect().ownership;
    record.run_id = Uuid::new_v4();
    record.session_id = other.id;
    record.execution_state = RecoveryExecutionState::Interrupted;
    record.admission_blocked = true;
    record.unresolved_effects = vec!["unknown_legacy_effect".into()];
    let payload = serde_json::to_string(&record).unwrap();
    store.connection.lock().unwrap().execute("INSERT INTO runtime_run_ownership (run_id,project_id,session_id,owner_generation,state,updated_at,payload) VALUES (?1,?2,?3,?4,'interrupted',0,?5)",params![record.run_id.to_string(),f.goal.project_id.to_string(),other.id.to_string(),record.owner_generation,&payload]).unwrap();
    assert!(f.inspect().recoverable);
    store.resolve_accounting_recovery(&f.decision()).unwrap();
    assert_eq!(
        store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM runtime_run_ownership WHERE run_id=?1",
                [record.run_id.to_string()],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        payload
    );
    store.connection.lock().unwrap().execute("UPDATE runtime_run_ownership SET session_id=?2,payload=json_set(payload,'$.session_id',?2) WHERE run_id=?1",params![record.run_id.to_string(),f.goal.session_id.to_string()]).unwrap();
    assert!(!f.inspect().recoverable);
    owner.retire().unwrap();
}

#[test]
fn owner_recovery_upgrades_legacy_accounting_and_old_writer_loss_fails_closed() {
    let f = Fixture::new();
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    // A genuine previous-format goal has no policy-charge fields or marker.
    store.connection.lock().unwrap().execute("UPDATE goals SET payload=json_remove(json_set(payload,'$.accounting.version',1),'$.accounting.disposition_format','$.accounting.counters.owner_accepted_tokens','$.accounting.counters.later_usage_excess_tokens') WHERE id=?1",[f.goal.id.to_string()]).unwrap();
    assert!(f.inspect().recoverable);
    store.resolve_accounting_recovery(&f.decision()).unwrap();
    let goal = store
        .goal(f.goal.project_id, f.goal.session_id)
        .unwrap()
        .unwrap();
    assert_eq!(goal.accounting.as_ref().unwrap().version, 2);
    assert!(goal.accounting.as_ref().unwrap().supported_format());
    // A legacy serializer would discard unknown v2 fields without changing the
    // version. The marker makes that loss an explicit stop, never a refund.
    store.connection.lock().unwrap().execute("UPDATE goals SET payload=json_remove(payload,'$.accounting.disposition_format','$.accounting.counters.owner_accepted_tokens','$.accounting.counters.later_usage_excess_tokens') WHERE id=?1",[f.goal.id.to_string()]).unwrap();
    let invalid = store
        .goal(f.goal.project_id, f.goal.session_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        invalid
            .accounting
            .as_ref()
            .unwrap()
            .remaining(invalid.token_budget)
            .reason,
        "unsupported_accounting_version"
    );
    assert!(matches!(
        store.admit_goal_run(
            f.goal.project_id,
            f.goal.session_id,
            GoalBinding {
                goal_id: f.goal.id,
                revision: invalid.accounting.unwrap().revision
            },
            Uuid::new_v4(),
            false
        ),
        Err(StoreError::GoalAccounting(
            "legacy_usage_history_unavailable"
        ))
    ));
    assert_eq!(
        f.inspect().charge.disposition.unwrap().client_charge_tokens,
        f.reservation
    );
    owner.retire().unwrap();
}

#[test]
fn owner_recovery_does_not_increase_exhausted_limits_or_complete_a_goal() {
    let f = Fixture::new();
    let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
    store
        .configure_goal_limits(
            f.goal.project_id,
            f.goal.session_id,
            f.goal.id,
            [f.reservation, 1, 1, 1, 60, f.reservation, 1, 1, 60],
        )
        .unwrap();
    store.resolve_accounting_recovery(&f.decision()).unwrap();
    let goal = store
        .goal(f.goal.project_id, f.goal.session_id)
        .unwrap()
        .unwrap();
    let account = goal.accounting.as_ref().unwrap();
    assert_eq!(
        account.remaining(goal.token_budget).estimated_tokens,
        Some(0)
    );
    assert!(goal.completion.is_none());
    assert_eq!(goal.lifecycle, GoalLifecycle::Active);
    assert!(matches!(
        store.admit_goal_run(
            f.goal.project_id,
            f.goal.session_id,
            GoalBinding {
                goal_id: goal.id,
                revision: account.revision
            },
            Uuid::new_v4(),
            false
        ),
        Err(StoreError::GoalAccounting("goal_budget_exhausted"))
    ));
    assert_eq!(f.inspect().goal_counters.request_attempts, 1);
    owner.retire().unwrap();
}

#[test]
fn owner_recovery_later_lower_usage_never_refunds_higher_usage_blocks_and_conflict_is_retained() {
    for total in [1, 154927, 154928] {
        let f = Fixture::new();
        let decision = f.decision();
        let (store, mut owner) = Store::open_for_host(&f.db).unwrap();
        store.resolve_accounting_recovery(&decision).unwrap();
        let usage = Fixture::usage(total);
        let settled = store
            .settle_goal_request(f.request, Some(usage.clone()))
            .unwrap();
        assert_eq!(
            store.settle_goal_request(f.request, Some(usage)).unwrap(),
            settled
        );
        assert_eq!(
            f.inspect().goal_counters.effective_tokens(),
            total.max(f.reservation)
        );
        let conflict = Fixture::usage(total + 1);
        assert!(store
            .settle_goal_request(f.request, Some(conflict.clone()))
            .is_err());
        assert_eq!(
            f.inspect()
                .charge
                .disposition
                .unwrap()
                .conflicting_later_usage,
            vec![conflict]
        );
        owner.retire().unwrap();
        assert_eq!(f.inspect().goal_counters.request_attempts, 1);
    }
}
