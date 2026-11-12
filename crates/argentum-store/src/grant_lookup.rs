//! Bounded authority candidates are separate from paginated grant history.
use super::*;
use argentum_domain::{ApprovalGrantHistoryCoverage, Capability};

// Execution audit records already bound admitted grant identities to 64.
pub const MAX_AUTHORIZATION_GRANTS: usize = 64;
const MAX_GRANT_BYTES: usize = 64 * 1024;

impl Store {
    /// Cancellation looks up only this run's still-unrevoked policy grants,
    /// rather than accidentally limiting revocation to a history display page.
    pub fn policy_run_grants(
        &self,
        project_id: ProjectId,
        run_id: RunId,
    ) -> Result<Vec<ApprovalGrantRecord>, StoreError> {
        self.scoped_run_grants(project_id, run_id, true)
    }

    /// Terminal tool cleanup retires run-scoped user/system grants as well.
    pub fn run_grants(
        &self,
        project_id: ProjectId,
        run_id: RunId,
    ) -> Result<Vec<ApprovalGrantRecord>, StoreError> {
        self.scoped_run_grants(project_id, run_id, false)
    }

    fn scoped_run_grants(
        &self,
        project_id: ProjectId,
        run_id: RunId,
        policy_only: bool,
    ) -> Result<Vec<ApprovalGrantRecord>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let issuer = if policy_only {
            " AND json_extract(payload, '$.issuer')='policy'"
        } else {
            ""
        };
        let sql = format!("SELECT length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload END FROM approval_grants WHERE project_id=?1 AND scope='run' AND revoked_at IS NULL AND json_extract(payload, '$.subject.run_id')=?2{issuer} ORDER BY id LIMIT 1001");
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query(params![project_id.to_string(), run_id.to_string()])?;
        let mut grants = Vec::new();
        while let Some(row) = rows.next()? {
            if grants.len() == 1000 {
                return Err(StoreError::InvalidRecord(
                    "run grant retirement coverage exceeds its bound".into(),
                ));
            }
            if row.get::<_, i64>(0)? > MAX_GRANT_BYTES as i64 {
                return Err(StoreError::InvalidRecord(
                    "stored approval grant exceeds the size limit".into(),
                ));
            }
            let encoded: String = row.get(1)?;
            let grant = serde_json::from_str::<ApprovalGrantRecord>(&encoded)?;
            validate_approval_grant(project_id, &grant)?;
            grants.push(grant);
        }
        Ok(grants)
    }
    /// Retrieve every potentially relevant grant, or fail closed on overflow.
    /// This is candidate selection, not an authorization decision. The caller
    /// must still run the existing authorizer and effect-time authority checks.
    pub fn authorization_grants(
        &self,
        project_id: ProjectId,
        capabilities: &[Capability],
        subjects: &[GrantSubject],
    ) -> Result<Vec<ApprovalGrantRecord>, StoreError> {
        if capabilities.is_empty() || capabilities.len() > 16 || subjects.len() > 6 {
            return Err(StoreError::InvalidRecord(
                "invalid grant lookup scope".into(),
            ));
        }
        if subjects.is_empty() {
            return Ok(Vec::new());
        }
        let mut values = vec![Value::Text(project_id.to_string())];
        let capability_sql = capabilities
            .iter()
            .map(|capability| {
                values.push(Value::Text(format!("{capability:?}")));
                "?"
            })
            .collect::<Vec<_>>()
            .join(",");
        let mut predicates = Vec::new();
        for subject in subjects {
            let encoded = serde_json::to_value(subject)?;
            // A literal enum scope also makes the scoped partial index usable.
            let mut predicate = format!("(scope = '{}'", grant_scope_name(subject.scope()));
            let fields: &[&str] = match subject {
                GrantSubject::Action { .. } => &["tool_id", "target", "binding"],
                GrantSubject::ToolCall { .. } => &["tool_call_id"],
                GrantSubject::Run { .. } => &["run_id"],
                GrantSubject::Session { .. } => &["session_id"],
                GrantSubject::Workspace { .. } => &["project_id"],
                GrantSubject::PersistentProfile { .. } => &["profile_id"],
            };
            for field in fields {
                let value = encoded[*field].as_str().ok_or_else(|| {
                    StoreError::InvalidRecord("invalid typed grant subject".into())
                })?;
                // Field names above are fixed Rust constants, never caller SQL.
                predicate.push_str(&format!(
                    " AND json_extract(payload, '$.subject.{field}') = ?"
                ));
                values.push(Value::Text(value.into()));
            }
            predicate.push(')');
            predicates.push(predicate);
        }
        let sql = format!(
            "SELECT length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= {MAX_GRANT_BYTES} THEN payload END
             FROM approval_grants WHERE project_id = ? AND capability IN ({capability_sql}) AND ({})
             ORDER BY id ASC LIMIT {}",
            predicates.join(" OR "), MAX_AUTHORIZATION_GRANTS + 1
        );
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query(params_from_iter(values))?;
        let mut grants = Vec::new();
        while let Some(row) = rows.next()? {
            if grants.len() == MAX_AUTHORIZATION_GRANTS {
                return Err(StoreError::InvalidRecord("authorization grant lookup exceeded its relevant-candidate bound; authority is incomplete".into()));
            }
            let bytes: i64 = row.get(0)?;
            if bytes > MAX_GRANT_BYTES as i64 {
                return Err(StoreError::InvalidRecord(
                    "stored approval grant exceeds the size limit".into(),
                ));
            }
            let encoded: String = row.get(1)?;
            let grant: ApprovalGrantRecord = serde_json::from_str(&encoded)?;
            validate_approval_grant(project_id, &grant)?;
            grants.push(grant);
        }
        Ok(grants)
    }

    /// A read snapshot gives the display page truthful completeness metadata.
    pub fn approval_grants_history_page(
        &self,
        project_id: ProjectId,
        limit: usize,
    ) -> Result<(Vec<ApprovalGrantRecord>, ApprovalGrantHistoryCoverage), StoreError> {
        validate_record_limit(limit)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let total: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM approval_grants WHERE project_id=?1",
            params![project_id.to_string()],
            |row| row.get(0),
        )?;
        let grants = {
            let mut statement = transaction.prepare("SELECT length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload END FROM approval_grants WHERE project_id=?1 ORDER BY created_at ASC,id ASC LIMIT ?2")?;
            let mut rows = statement.query(params![project_id.to_string(), limit as i64])?;
            let mut grants = Vec::new();
            while let Some(row) = rows.next()? {
                if row.get::<_, i64>(0)? > MAX_GRANT_BYTES as i64 {
                    return Err(StoreError::InvalidRecord(
                        "stored approval grant exceeds the size limit".into(),
                    ));
                }
                let encoded: String = row.get(1)?;
                let grant: ApprovalGrantRecord = serde_json::from_str(&encoded)?;
                validate_approval_grant(project_id, &grant)?;
                grants.push(grant);
            }
            grants
        };
        transaction.commit()?;
        let total_count = u64::try_from(total)
            .map_err(|_| StoreError::InvalidRecord("invalid grant history count".into()))?;
        let coverage = ApprovalGrantHistoryCoverage {
            limit: limit as u64,
            returned: grants.len() as u64,
            total_count,
            complete: grants.len() as u64 == total_count,
        };
        Ok((grants, coverage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::GrantIssuer;

    fn grant(
        subject: GrantSubject,
        capability: Capability,
        created_at: OffsetDateTime,
    ) -> ApprovalGrantRecord {
        ApprovalGrantRecord {
            id: Uuid::new_v4(),
            audit_id: Uuid::new_v4(),
            capability,
            scope: subject.scope(),
            subject,
            created_at,
            expires_at: None,
            issuer: GrantIssuer::User,
            reason: "Synthetic scoped lookup control".into(),
            revoked_at: None,
        }
    }

    fn action() -> GrantSubject {
        GrantSubject::Action {
            tool_id: "verify".into(),
            target: "fixed runners: project:lookup-control".into(),
            binding: Sha256Digest("1".repeat(64)),
        }
    }

    fn irrelevant(store: &Store, project: ProjectId, count: usize, at: OffsetDateTime) {
        for _ in 0..count {
            store
                .save_approval_grant(
                    project,
                    &grant(
                        GrantSubject::ToolCall {
                            tool_call_id: Uuid::new_v4(),
                        },
                        Capability::ExternalProcess,
                        at,
                    ),
                )
                .unwrap();
        }
    }

    #[test]
    fn exact_authority_lookup_finds_record_1618_and_old_matching_grants_after_reload() {
        for early in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let workspace = root.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let database = root.path().join("state.sqlite3");
            let store = Store::open(&database).unwrap();
            let project = store
                .resolve_workspace(&workspace)
                .unwrap()
                .snapshot
                .project
                .id;
            let at = now() - time::Duration::hours(2);
            let older = grant(action(), Capability::ExternalProcess, at);
            if early {
                store.save_approval_grant(project, &older).unwrap();
            }
            irrelevant(&store, project, 1617, at + time::Duration::minutes(1));
            let later = grant(
                action(),
                Capability::ExternalProcess,
                at + time::Duration::minutes(2),
            );
            store.save_approval_grant(project, &later).unwrap();
            let (page, coverage) = store.approval_grants_history_page(project, 1000).unwrap();
            assert_eq!(page.len(), 1000);
            assert_eq!(coverage.total_count, if early { 1619 } else { 1618 });
            assert!(!coverage.complete);
            assert!(!page.iter().any(|record| record.id == later.id));
            drop(store);
            let reopened = Store::open(&database).unwrap();
            let found = reopened
                .authorization_grants(project, &[Capability::ExternalProcess], &[action()])
                .unwrap();
            assert_eq!(found.len(), if early { 2 } else { 1 });
            assert!(found.iter().any(|record| record.id == later.id));
            if early {
                assert!(found.iter().any(|record| record.id == older.id));
            }
            assert_eq!(
                reopened
                    .approval_grants_history_page(project, 1000)
                    .unwrap()
                    .1,
                coverage
            );
        }
    }

    #[test]
    fn equal_ordering_timestamps_do_not_affect_exact_resolution() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let project = store
            .resolve_workspace(root.path())
            .unwrap()
            .snapshot
            .project
            .id;
        let at = now() - time::Duration::minutes(1);
        for index in 1..=1617 {
            let mut record = grant(
                GrantSubject::ToolCall {
                    tool_call_id: Uuid::new_v4(),
                },
                Capability::ExternalProcess,
                at,
            );
            record.id = Uuid::from_u128(index);
            store.save_approval_grant(project, &record).unwrap();
        }
        let mut matching = grant(action(), Capability::ExternalProcess, at);
        matching.id = Uuid::from_u128(2000);
        store.save_approval_grant(project, &matching).unwrap();
        assert!(!store
            .approval_grants(project, 1000)
            .unwrap()
            .iter()
            .any(|g| g.id == matching.id));
        assert_eq!(
            store
                .authorization_grants(project, &[Capability::ExternalProcess], &[action()])
                .unwrap(),
            vec![matching]
        );
    }

    #[test]
    fn foreign_projects_capabilities_and_changed_bindings_are_not_candidates() {
        let root = tempfile::tempdir().unwrap();
        let foreign = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let project = store
            .resolve_workspace(root.path())
            .unwrap()
            .snapshot
            .project
            .id;
        let other = store
            .resolve_workspace(foreign.path())
            .unwrap()
            .snapshot
            .project
            .id;
        let at = now() - time::Duration::minutes(1);
        store
            .save_approval_grant(other, &grant(action(), Capability::ExternalProcess, at))
            .unwrap();
        store
            .save_approval_grant(project, &grant(action(), Capability::ReadFiles, at))
            .unwrap();
        let mut changed = action();
        if let GrantSubject::Action { binding, .. } = &mut changed {
            *binding = Sha256Digest("2".repeat(64));
        }
        store
            .save_approval_grant(project, &grant(changed, Capability::ExternalProcess, at))
            .unwrap();
        assert!(store
            .authorization_grants(project, &[Capability::ExternalProcess], &[action()])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn every_scope_uses_exact_subject_identity_and_relevant_overflow_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let project = store
            .resolve_workspace(root.path())
            .unwrap()
            .snapshot
            .project
            .id;
        let subjects = [
            action(),
            GrantSubject::ToolCall {
                tool_call_id: Uuid::new_v4(),
            },
            GrantSubject::Run {
                run_id: Uuid::new_v4(),
            },
            GrantSubject::Session {
                session_id: Uuid::new_v4(),
            },
            GrantSubject::Workspace {
                project_id: project,
            },
            GrantSubject::PersistentProfile {
                profile_id: "verification".into(),
            },
        ];
        let at = now() - time::Duration::minutes(1);
        for subject in &subjects {
            store
                .save_approval_grant(
                    project,
                    &grant(subject.clone(), Capability::ExternalProcess, at),
                )
                .unwrap();
        }
        assert_eq!(
            store
                .authorization_grants(project, &[Capability::ExternalProcess], &subjects)
                .unwrap()
                .len(),
            6
        );
        for _ in 6..=MAX_AUTHORIZATION_GRANTS {
            store
                .save_approval_grant(project, &grant(action(), Capability::ExternalProcess, at))
                .unwrap();
        }
        let error = store
            .authorization_grants(project, &[Capability::ExternalProcess], &subjects)
            .unwrap_err();
        assert!(error.to_string().contains("authority is incomplete"));
        assert_eq!(
            store
                .approval_grants_history_page(project, 1000)
                .unwrap()
                .1
                .total_count,
            65
        );
    }

    #[test]
    fn policy_run_retirement_is_independent_of_history_and_excludes_foreign_or_user_grants() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let project = store
            .resolve_workspace(root.path())
            .unwrap()
            .snapshot
            .project
            .id;
        let at = now() - time::Duration::minutes(1);
        irrelevant(&store, project, 1617, at);
        let run_id = Uuid::new_v4();
        let mut policy = grant(
            GrantSubject::Run { run_id },
            Capability::ExternalProcess,
            at,
        );
        policy.issuer = GrantIssuer::Policy;
        store.save_approval_grant(project, &policy).unwrap();
        let user = grant(
            GrantSubject::Run { run_id },
            Capability::ExternalProcess,
            at,
        );
        store.save_approval_grant(project, &user).unwrap();
        let mut foreign = policy.clone();
        foreign.id = Uuid::new_v4();
        foreign.subject = GrantSubject::Run {
            run_id: Uuid::new_v4(),
        };
        store.save_approval_grant(project, &foreign).unwrap();
        assert_eq!(
            store.policy_run_grants(project, run_id).unwrap(),
            vec![policy.clone()]
        );
        let all = store.run_grants(project, run_id).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&user) && all.contains(&policy));
        store
            .revoke_approval_grant(project, policy.id, now())
            .unwrap();
        assert!(store.policy_run_grants(project, run_id).unwrap().is_empty());
    }
}
