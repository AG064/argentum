//! The desktop composer path for goals: set a goal, run tasks toward it, and
//! pause, resume and clear it, through `AppCommand` only.

use super::*;

const OBJECTIVE: &str = "Keep the changelog current";

struct UsageProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    /// Arguments for a report_goal_complete call on a run's first request.
    report: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl ModelProvider for UsageProvider {
    fn id(&self) -> &'static str {
        "goal-composer"
    }
    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }
    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let first = request.tool_exchanges.is_empty();
        let offered = request
            .tools
            .iter()
            .any(|tool| tool.name == "report_goal_complete");
        self.requests.lock().unwrap().push(request);
        let report = self.report.lock().unwrap().clone();
        if let Some(arguments) = report.filter(|_| first && offered) {
            sender
                .send(ProviderEvent::ToolCall(ModelToolCall {
                    id: "report-1".into(),
                    name: "report_goal_complete".into(),
                    arguments,
                }))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        for event in [
            ProviderEvent::UsageObservation(argentum_domain::goal_accounting::ObservedUsage {
                input_tokens: None,
                output_tokens: None,
                total_tokens: 40,
                cached_input_tokens: None,
                reasoning_tokens: None,
            }),
            ProviderEvent::Delta("Done.".into()),
            ProviderEvent::Completed,
        ] {
            sender
                .send(event)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        Ok(())
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    runtime: TestRuntime,
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    report: Arc<Mutex<Option<String>>>,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap(),
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let report = Arc::new(Mutex::new(None));
        let mut providers = ProviderRegistry::default();
        providers.register(UsageProvider {
            requests: requests.clone(),
            report: report.clone(),
        });
        let runtime = test_runtime(
            Store::open_in_memory().unwrap(),
            providers,
            ToolRegistry::with_builtins(workspace.clone()),
            workspace,
        );
        configure_selected_provider(&runtime, "goal-composer").await;
        Self {
            _directory: directory,
            runtime,
            requests,
            report,
        }
    }

    fn script_report(&self, arguments: &str) {
        *self.report.lock().unwrap() = Some(arguments.into());
    }

    fn last_request(&self) -> ModelRequest {
        self.requests
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a request")
    }

    async fn ordinary(&self) {
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.runtime.dispatch(AppCommand::SubmitTask {
                prompt: "An unrelated question".into(),
            }),
        )
        .await
        .expect("watchdog")
        .expect("ordinary task");
    }

    async fn set_goal(&self) -> Goal {
        self.runtime
            .dispatch(AppCommand::SetGoal {
                objective: OBJECTIVE.into(),
                token_budget: None,
                tool_budget: None,
                time_budget_seconds: None,
            })
            .await
            .expect("a goal without limits is accepted");
        self.runtime.goal().unwrap().expect("goal")
    }

    async fn run(&self, goal_id: argentum_domain::GoalId) -> Result<(), RuntimeError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.runtime.dispatch(AppCommand::SubmitGoalTask {
                goal_id,
                prompt: "Add the latest entry".into(),
            }),
        )
        .await
        .expect("goal task watchdog")
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    fn goal(&self) -> Option<Goal> {
        self.runtime.goal().unwrap()
    }
}

#[tokio::test]
async fn composer_goal_lifecycle_runs_tasks_toward_the_goal() {
    let fixture = Fixture::new().await;
    let goal = fixture.set_goal().await;
    assert_eq!(goal.lifecycle, GoalLifecycle::Active);
    assert_eq!(goal.iteration, 0);

    fixture.run(goal.id).await.expect("first goal task");
    let after_first = fixture.goal().expect("goal after the first task");
    assert_eq!(
        after_first.iteration, 1,
        "the run counts as a goal iteration"
    );
    assert_eq!(after_first.tokens_used, 40, "observed usage is charged");
    assert_eq!(fixture.request_count(), 1);
    let request = format!("{:?}", fixture.requests.lock().unwrap()[0]);
    assert!(
        request.contains(OBJECTIVE),
        "the model request carries the goal objective"
    );

    fixture
        .runtime
        .dispatch(AppCommand::PauseGoal)
        .await
        .expect("pause");
    assert_eq!(fixture.goal().unwrap().lifecycle, GoalLifecycle::Paused);
    assert!(matches!(
        fixture.run(goal.id).await,
        Err(RuntimeError::GoalNotActive)
    ));
    assert_eq!(fixture.request_count(), 1, "a paused goal sends nothing");
    assert!(matches!(
        fixture.runtime.dispatch(AppCommand::PauseGoal).await,
        Err(RuntimeError::GoalNotResumable)
    ));

    fixture
        .runtime
        .dispatch(AppCommand::ResumeGoal)
        .await
        .expect("resume");
    assert_eq!(fixture.goal().unwrap().lifecycle, GoalLifecycle::Active);
    fixture.run(goal.id).await.expect("second goal task");
    let after_second = fixture.goal().unwrap();
    assert_eq!(after_second.iteration, 2);
    assert_eq!(after_second.tokens_used, 80);
    assert_eq!(after_second.id, goal.id, "the goal identity is kept");

    fixture
        .runtime
        .dispatch(AppCommand::ClearGoal)
        .await
        .expect("clear");
    assert_eq!(fixture.goal(), None);
    assert!(matches!(
        fixture.run(goal.id).await,
        Err(RuntimeError::GoalMissing)
    ));
    assert_eq!(fixture.request_count(), 2);
}

#[tokio::test]
async fn composer_goal_task_never_substitutes_a_replaced_goal() {
    let fixture = Fixture::new().await;
    let first = fixture.set_goal().await;
    fixture
        .runtime
        .dispatch(AppCommand::ClearGoal)
        .await
        .expect("clear");
    let second = fixture.set_goal().await;
    assert_ne!(first.id, second.id);

    assert!(matches!(
        fixture.run(first.id).await,
        Err(RuntimeError::SubmissionTargetChanged)
    ));
    assert_eq!(fixture.request_count(), 0);
    assert_eq!(fixture.goal().unwrap().iteration, 0);
}

#[tokio::test]
async fn composer_goal_task_respects_the_goal_token_limit() {
    let fixture = Fixture::new().await;
    fixture
        .runtime
        .dispatch(AppCommand::SetGoal {
            objective: OBJECTIVE.into(),
            token_budget: Some(1),
            tool_budget: None,
            time_budget_seconds: None,
        })
        .await
        .expect("goal with a token limit");
    let goal = fixture.goal().unwrap();

    assert!(fixture.run(goal.id).await.is_err());
    assert_eq!(
        fixture.request_count(),
        0,
        "an exhausted allowance stops the run before any model request"
    );
}

#[tokio::test]
async fn ordinary_submissions_stay_outside_the_goal() {
    let fixture = Fixture::new().await;
    fixture.set_goal().await;
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        fixture.runtime.dispatch(AppCommand::SubmitTask {
            prompt: "An unrelated question".into(),
        }),
    )
    .await
    .expect("watchdog")
    .expect("ordinary task");
    let goal = fixture.goal().unwrap();
    assert_eq!(goal.iteration, 0);
    assert_eq!(goal.tokens_used, 0);
}

fn offers_report_tool(request: &ModelRequest) -> bool {
    request
        .tools
        .iter()
        .any(|tool| tool.name == "report_goal_complete")
}

fn goal_context(request: &ModelRequest) -> Option<serde_json::Value> {
    request.history.iter().find_map(|message| {
        let value: serde_json::Value = serde_json::from_str(&message.content).ok()?;
        matches!(
            value["kind"].as_str(),
            Some("session_goal_context" | "explicit_goal_task_guidance")
        )
        .then_some(value)
    })
}

#[tokio::test]
async fn every_run_in_a_session_with_an_active_goal_receives_the_objective_as_data() {
    let fixture = Fixture::new().await;
    fixture.ordinary().await;
    assert_eq!(goal_context(&fixture.last_request()), None, "no goal yet");

    let goal = fixture.set_goal().await;
    fixture.ordinary().await;
    let ordinary = fixture.last_request();
    let context = goal_context(&ordinary).expect("ordinary run sees the goal");
    assert_eq!(context["kind"], "session_goal_context");
    assert_eq!(context["objective"], OBJECTIVE);
    assert_eq!(context["this_run_counts_toward_goal"], false);
    assert!(
        !offers_report_tool(&ordinary),
        "only a goal-bound run may report the goal complete"
    );
    assert_eq!(fixture.goal().unwrap().iteration, 0);

    fixture.run(goal.id).await.expect("goal task");
    let bound = fixture.last_request();
    let guidance = goal_context(&bound).expect("goal-bound run sees the goal");
    assert_eq!(guidance["kind"], "explicit_goal_task_guidance");
    assert_eq!(guidance["objective"], OBJECTIVE);
    assert!(offers_report_tool(&bound));

    fixture
        .runtime
        .dispatch(AppCommand::PauseGoal)
        .await
        .expect("pause");
    fixture.ordinary().await;
    assert_eq!(
        goal_context(&fixture.last_request()),
        None,
        "a paused goal is not sent"
    );
}

#[tokio::test]
async fn the_model_can_report_a_goal_done_but_only_the_owner_completes_it() {
    let fixture = Fixture::new().await;
    let goal = fixture.set_goal().await;
    fixture.script_report(r#"{"summary":"Added the 0.1.0 entry."}"#);
    fixture.run(goal.id).await.expect("goal task with a report");

    let reported = fixture.goal().unwrap();
    assert_eq!(
        reported.lifecycle,
        GoalLifecycle::Active,
        "a report never completes the goal by itself"
    );
    let report = reported.completion_report.clone().expect("report recorded");
    assert_eq!(report.summary, "Added the 0.1.0 entry.");
    let told = format!("{:?}", fixture.last_request().tool_exchanges);
    assert!(
        told.contains("goal_completed") && told.contains("recorded"),
        "the model is told the report was recorded and the goal is not complete"
    );

    // Dismissing keeps the goal active and drops the report.
    fixture
        .runtime
        .dispatch(AppCommand::DismissGoalReport { goal_id: goal.id })
        .await
        .expect("dismiss");
    let dismissed = fixture.goal().unwrap();
    assert_eq!(dismissed.lifecycle, GoalLifecycle::Active);
    assert_eq!(dismissed.completion_report, None);
    assert!(matches!(
        fixture
            .runtime
            .dispatch(AppCommand::ConfirmGoalReport { goal_id: goal.id })
            .await,
        Err(RuntimeError::GoalReportMissing)
    ));

    // A second report, confirmed by the owner, completes the goal.
    fixture.run(goal.id).await.expect("second goal task");
    assert!(fixture.goal().unwrap().completion_report.is_some());
    fixture
        .runtime
        .dispatch(AppCommand::ConfirmGoalReport { goal_id: goal.id })
        .await
        .expect("confirm");
    let complete = fixture.goal().unwrap();
    assert_eq!(complete.lifecycle, GoalLifecycle::Complete);
    assert!(matches!(
        fixture.run(goal.id).await,
        Err(RuntimeError::GoalNotActive)
    ));
}

#[tokio::test]
async fn a_bad_report_is_refused_to_the_model_without_failing_the_run() {
    let fixture = Fixture::new().await;
    let goal = fixture.set_goal().await;
    fixture.script_report(r#"{"summary":"Done.","extra":1}"#);
    fixture.run(goal.id).await.expect("the run still completes");
    let goal = fixture.goal().unwrap();
    assert_eq!(goal.completion_report, None);
    assert_eq!(goal.lifecycle, GoalLifecycle::Active);
}

#[tokio::test]
async fn owner_report_decisions_name_the_current_goal() {
    let fixture = Fixture::new().await;
    let goal = fixture.set_goal().await;
    fixture.script_report(r#"{"summary":"Done."}"#);
    fixture.run(goal.id).await.expect("goal task");
    assert!(matches!(
        fixture
            .runtime
            .dispatch(AppCommand::ConfirmGoalReport {
                goal_id: argentum_domain::GoalId::new_v4()
            })
            .await,
        Err(RuntimeError::SubmissionTargetChanged)
    ));
    assert_eq!(fixture.goal().unwrap().lifecycle, GoalLifecycle::Active);
}
