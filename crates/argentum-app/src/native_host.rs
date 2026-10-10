#[cfg(test)]
mod tests {
    use argentum_cli::{CommandHost, HostConfig, LocalCommandHandle};
    use argentum_domain::{AppCommand, ProviderKind, ProviderProfile};
    use argentum_platform::{PlatformError, SecretStore};
    use argentum_security::SecretValue;
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use tokio::sync::{mpsc, oneshot};
    use tokio::time::{timeout, Duration};
    use tokio_util::sync::CancellationToken;

    use super::{
        clear_local_provider_credential, handle_relay_failure, save_local_provider_credential,
        seed_missing_profiles, shutdown_native_owner, NativeCredentialMutationError,
        NativeHostError, NativeOwner, NativeTaskGroup, RelayLifecycle,
    };

    #[derive(Default)]
    struct MemorySecretStore {
        values: Mutex<HashMap<String, SecretValue>>,
    }

    struct DropProbe(Arc<AtomicBool>);

    struct FakeNativeOwner {
        request_calls: Arc<AtomicUsize>,
        shutdown_calls: Arc<AtomicUsize>,
        retire_calls: Arc<AtomicUsize>,
        launch_count: Arc<AtomicUsize>,
        failures_remaining: usize,
        retired: bool,
        request_signal: Option<oneshot::Sender<()>>,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    impl FakeNativeOwner {
        fn new(failures_remaining: usize) -> Self {
            Self {
                request_calls: Arc::new(AtomicUsize::new(0)),
                shutdown_calls: Arc::new(AtomicUsize::new(0)),
                retire_calls: Arc::new(AtomicUsize::new(0)),
                launch_count: Arc::new(AtomicUsize::new(1)),
                failures_remaining,
                retired: false,
                request_signal: None,
                order: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn record(&self, step: &'static str) {
            self.order
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(step);
        }
    }

    impl NativeOwner for FakeNativeOwner {
        type Error = ();

        fn request_shutdown(&mut self) -> Result<(), Self::Error> {
            self.request_calls.fetch_add(1, Ordering::SeqCst);
            self.record("request-shutdown");
            if let Some(request_signal) = self.request_signal.take() {
                let _ = request_signal.send(());
            }
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), Self::Error> {
            let attempt = self.shutdown_calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.record(if attempt == 1 {
                "host-shutdown-1"
            } else {
                "host-shutdown-2"
            });
            if !self.retired {
                self.retired = true;
                self.retire_calls.fetch_add(1, Ordering::SeqCst);
            }
            if self.failures_remaining > 0 {
                self.failures_remaining -= 1;
                Err(())
            } else {
                Ok(())
            }
        }
    }

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    impl SecretStore for MemorySecretStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            Ok(self
                .values
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(key)
                .cloned())
        }

        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.values
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(key.to_owned(), value);
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.values
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(key);
            Ok(())
        }
    }

    fn profile(id: &str, label: &str, selected: bool) -> ProviderProfile {
        ProviderProfile {
            id: id.into(),
            label: label.into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: format!("https://{id}.example/v1/"),
            model: format!("{id}-model"),
            selected,
        }
    }

    #[tokio::test]
    async fn closing_native_admission_drains_existing_work_and_rejects_new_work() {
        let group = NativeTaskGroup::new(tokio::runtime::Handle::current());
        let cancellation = group.cancellation_token();
        let (release_tx, release_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();

        assert!(group.spawn(async move {
            let _ = release_rx.await;
            let _ = done_tx.send(());
        }));
        group.close();
        assert!(cancellation.is_cancelled());
        let rejected_ran = Arc::new(AtomicBool::new(false));
        let rejected_ran_in_task = Arc::clone(&rejected_ran);
        assert!(!group.spawn(async move {
            rejected_ran_in_task.store(true, Ordering::SeqCst);
        }));

        release_tx.send(()).expect("release admitted task");
        group.wait().await;
        tokio::task::yield_now().await;
        done_rx.await.expect("admitted task drained");
        assert!(!rejected_ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn rejected_native_credential_admission_drops_owned_secret_payload() {
        let group = NativeTaskGroup::new(tokio::runtime::Handle::current());
        group.close();
        let dropped = Arc::new(AtomicBool::new(false));
        let drop_probe = DropProbe(Arc::clone(&dropped));
        let credential = SecretValue::new("rejected-native-credential-fixture");

        assert!(!group.spawn(async move {
            let _credential = credential;
            let _drop_probe = drop_probe;
        }));
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    fn seeding_profiles_preserves_selection_and_ignores_existing_and_duplicate_ids() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = runtime
            .block_on(CommandHost::start_composed(HostConfig::in_memory(
                workspace.path(),
            )))
            .expect("host");
        let client = host.client();
        runtime
            .block_on(client.dispatch(AppCommand::SaveProviderProfile {
                profile: profile("existing-seed-test", "Existing original", false),
            }))
            .expect("save existing profile");
        let selected_before = host
            .provider_profiles()
            .expect("profiles before seed")
            .into_iter()
            .find(|profile| profile.selected)
            .map(|profile| profile.id);

        seed_missing_profiles(
            &runtime,
            &host,
            vec![
                profile("selected-seed-test", "Selected seed", true),
                profile("duplicate-seed-test", "First duplicate", false),
                profile("duplicate-seed-test", "Second duplicate", false),
                profile("existing-seed-test", "Existing replacement", false),
            ],
        )
        .expect("seed profiles");

        let profiles = host.provider_profiles().expect("profiles after seed");
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.selected)
                .map(|profile| profile.id.clone()),
            selected_before
        );
        assert!(
            !profiles
                .iter()
                .find(|profile| profile.id == "selected-seed-test")
                .expect("selected input was seeded")
                .selected
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.id == "duplicate-seed-test")
                .expect("duplicate input was seeded")
                .label,
            "First duplicate"
        );
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.id == "existing-seed-test")
                .expect("existing profile remains")
                .label,
            "Existing original"
        );

        let group = NativeTaskGroup::new(runtime.handle().clone());
        runtime
            .block_on(shutdown_native_owner(
                &mut host,
                &group,
                RelayLifecycle::new(),
                CancellationToken::new(),
                None,
            ))
            .expect("shutdown host");
    }

    #[tokio::test]
    async fn native_shutdown_empties_the_composed_lifecycle() {
        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("host");
        let observer = host.lifecycle_observer();
        let group = NativeTaskGroup::new(tokio::runtime::Handle::current());

        shutdown_native_owner(
            &mut host,
            &group,
            RelayLifecycle::new(),
            CancellationToken::new(),
            None,
        )
        .await
        .expect("complete shutdown");

        assert!(observer
            .harness_lifecycle_snapshot()
            .await
            .expect("lifecycle")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn native_shutdown_drains_commands_while_the_relay_is_active() {
        let group = NativeTaskGroup::new(tokio::runtime::Handle::current());
        let lifecycle = RelayLifecycle::new();
        let cancellation = CancellationToken::new();
        let (request_tx, request_rx) = oneshot::channel();
        let (terminal_tx, mut terminal_rx) = mpsc::channel(1);
        let (projected_tx, projected_rx) = oneshot::channel();
        let mut owner = FakeNativeOwner::new(0);
        owner.request_signal = Some(request_tx);
        let order = Arc::clone(&owner.order);
        let command_order = Arc::clone(&order);
        assert!(group.spawn(async move {
            request_rx.await.expect("shutdown request");
            command_order
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("command-terminal");
            terminal_tx.send(()).await.expect("terminal event");
            projected_rx.await.expect("terminal event projected");
            command_order
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("command-drained");
        }));

        let relay_order = Arc::clone(&order);
        let relay_cancellation = cancellation.clone();
        let relay = tokio::spawn(async move {
            terminal_rx.recv().await.expect("terminal event received");
            relay_order
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("relay-terminal");
            projected_tx.send(()).expect("terminal event projection");
            relay_cancellation.cancelled().await;
            relay_order
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("relay-cancelled");
            Ok(())
        });

        timeout(
            Duration::from_secs(2),
            shutdown_native_owner(&mut owner, &group, lifecycle, cancellation, Some(relay)),
        )
        .await
        .expect("ordered shutdown did not deadlock")
        .expect("ordered shutdown");

        assert_eq!(
            *order
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![
                "request-shutdown",
                "command-terminal",
                "relay-terminal",
                "command-drained",
                "relay-cancelled",
                "host-shutdown-1",
            ]
        );
    }

    #[tokio::test]
    async fn native_shutdown_retries_the_same_owner_until_cleanup_succeeds() {
        let group = NativeTaskGroup::new(tokio::runtime::Handle::current());
        let mut owner = FakeNativeOwner::new(3);

        timeout(
            Duration::from_secs(2),
            shutdown_native_owner(
                &mut owner,
                &group,
                RelayLifecycle::new(),
                CancellationToken::new(),
                None,
            ),
        )
        .await
        .expect("same-owner recovery remained bounded by the test timeout")
        .expect("same-owner retry completes cleanup");

        assert_eq!(owner.request_calls.load(Ordering::SeqCst), 1);
        assert_eq!(owner.shutdown_calls.load(Ordering::SeqCst), 4);
        assert_eq!(owner.retire_calls.load(Ordering::SeqCst), 1);
        assert_eq!(owner.launch_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn native_credential_mutations_use_only_the_local_handle_and_redact_failures() {
        let typed_save_helper: fn(
            &LocalCommandHandle,
            String,
            SecretValue,
        ) -> Result<(), NativeCredentialMutationError> = save_local_provider_credential;
        let workspace = tempfile::tempdir().expect("workspace");
        let config = HostConfig::in_memory(workspace.path())
            .with_secret_store(Arc::new(MemorySecretStore::default()));
        let mut host = CommandHost::start_composed(config).await.expect("host");
        let local = host.local_handle();
        let fixture = "native-connector-credential-that-must-not-leak";

        typed_save_helper(&local, "deepseek".into(), SecretValue::new(fixture))
            .expect("save credential through local handle");
        assert!(local.provider_credential_configured("deepseek"));
        clear_local_provider_credential(&local, "deepseek".into())
            .expect("clear credential through local handle");
        assert!(!local.provider_credential_configured("deepseek"));

        let error = typed_save_helper(&local, "custom".into(), SecretValue::new(fixture))
            .expect_err("unsupported provider must fail");
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains(fixture));
        assert!(!rendered.contains("custom"));

        let group = NativeTaskGroup::new(tokio::runtime::Handle::current());
        shutdown_native_owner(
            &mut host,
            &group,
            RelayLifecycle::new(),
            CancellationToken::new(),
            None,
        )
        .await
        .expect("shutdown host");
    }

    #[test]
    fn unexpected_relay_failure_while_ui_is_active_reports_and_quits() {
        let lifecycle = RelayLifecycle::new();
        let reported = Arc::new(AtomicBool::new(false));
        let reported_by_notifier = Arc::clone(&reported);

        let result = handle_relay_failure(&lifecycle, move || {
            reported_by_notifier.store(true, Ordering::SeqCst);
        });

        assert!(matches!(result, Err(NativeHostError::EventRelay)));
        assert!(reported.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn owner_shutdown_claim_wins_before_relay_failure_without_reporting() {
        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("host");
        let group = NativeTaskGroup::new(tokio::runtime::Handle::current());
        let lifecycle = RelayLifecycle::new();
        let cancellation = CancellationToken::new();
        let relay_lifecycle = lifecycle.clone();
        let reported = Arc::new(AtomicBool::new(false));
        let reported_by_relay = Arc::clone(&reported);
        let (release_relay_tx, release_relay_rx) = oneshot::channel();
        let relay = tokio::spawn(async move {
            release_relay_rx.await.expect("release relay");
            handle_relay_failure(&relay_lifecycle, || {
                reported_by_relay.store(true, Ordering::SeqCst);
            })
        });

        assert!(lifecycle.begin_owner_shutdown());
        release_relay_tx.send(()).expect("release relay failure");
        shutdown_native_owner(&mut host, &group, lifecycle, cancellation, Some(relay))
            .await
            .expect("owner shutdown is clean");
        assert!(!reported.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn relay_failure_claim_wins_before_owner_and_cleanup_preserves_failure() {
        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("host");
        let group = NativeTaskGroup::new(tokio::runtime::Handle::current());
        let lifecycle = RelayLifecycle::new();
        let cancellation = CancellationToken::new();
        let relay_lifecycle = lifecycle.clone();
        let (failure_claimed_tx, failure_claimed_rx) = oneshot::channel();
        let relay = tokio::spawn(async move {
            handle_relay_failure(&relay_lifecycle, || {
                failure_claimed_tx.send(()).expect("signal failure claim");
            })
        });

        failure_claimed_rx.await.expect("relay claimed failure");
        assert!(!lifecycle.begin_owner_shutdown());
        let error = shutdown_native_owner(&mut host, &group, lifecycle, cancellation, Some(relay))
            .await
            .expect_err("relay failure remains a cleanup failure");

        assert!(matches!(
            error,
            NativeHostError::Cleanup(steps)
                if steps == vec![super::NativeCleanupStep::EventRelay]
        ));
    }
}
use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use argentum_cli::{CommandHost, CommandHostObserver, HostConfig, LocalCommandHandle};
use argentum_domain::{AppCommand, AppEvent, ProviderProfile};
use argentum_security::SecretValue;
use argentum_ui::{connect_commands, connect_provider_credentials, UiHandle, WeakUiHandle};
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::ui_bridge::{
    deliver_ui_command_failure, deliver_ui_event, deliver_ui_local_inference_state,
    deliver_ui_provider_credential_failure, deliver_ui_provider_credential_success,
    deliver_ui_relay_failure_and_quit, next_coalesced_event, republish_after_lag, CoalescedEvent,
};

pub struct NativeHostOptions {
    pub workspace_label: String,
    pub seed_profiles: Vec<ProviderProfile>,
    pub inference_foreground: Option<Arc<dyn NativeInferenceForeground>>,
}

pub trait NativeInferenceForeground: Send + Sync {
    fn start_generation(&self, generation: u64) -> bool;
    fn stop_generation(&self, generation: u64) -> bool;
    fn stop_active(&self) -> bool;
}

#[derive(Debug)]
pub enum NativeHostError {
    Runtime,
    Host,
    Ui,
    EventLoop,
    EventRelay,
    Cleanup(Vec<NativeCleanupStep>),
    ApplicationAndCleanup {
        application: Box<NativeHostError>,
        cleanup: Box<NativeHostError>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCleanupStep {
    RequestShutdown,
    EventRelay,
    HostShutdown,
}

impl fmt::Display for NativeHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime => formatter.write_str("Argentum could not start its native runtime."),
            Self::Host => {
                formatter.write_str("Argentum could not prepare the native command host.")
            }
            Self::Ui | Self::EventLoop => {
                formatter.write_str("Argentum could not run the native interface.")
            }
            Self::EventRelay => {
                formatter.write_str("Argentum could not relay native interface events.")
            }
            Self::Cleanup(_) => formatter.write_str("Argentum could not complete native cleanup."),
            Self::ApplicationAndCleanup { .. } => formatter
                .write_str("Argentum encountered an application error and a cleanup error."),
        }
    }
}

impl std::error::Error for NativeHostError {}

pub fn run_native_host<F>(
    config: HostConfig,
    options: NativeHostOptions,
    configure_ui: F,
) -> Result<(), NativeHostError>
where
    F: FnOnce(&UiHandle),
{
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| NativeHostError::Runtime)?;
    let mut host = runtime
        .block_on(CommandHost::start_composed(config))
        .map_err(|_| NativeHostError::Host)?;
    let mut session = NativeHostSession::new(runtime.handle().clone());
    let inference_foreground = options.inference_foreground.clone();

    let application =
        run_event_loop_with_owner(&runtime, &mut host, options, configure_ui, &mut session);
    let cleanup = runtime.block_on(shutdown_native_owner(
        &mut host,
        &session.command_tasks,
        session.relay_lifecycle.clone(),
        session.relay_cancellation.clone(),
        session.relay.take(),
    ));
    if let Some(foreground) = inference_foreground {
        if !foreground.stop_active() {
            tracing::warn!("Android inference foreground cleanup was not confirmed");
        }
    }

    drop(host);
    drop(runtime);
    combine_application_and_cleanup(application, cleanup)
}

struct NativeHostSession {
    command_tasks: NativeTaskGroup,
    relay_lifecycle: RelayLifecycle,
    relay_cancellation: CancellationToken,
    relay: Option<JoinHandle<Result<(), NativeHostError>>>,
}

impl NativeHostSession {
    fn new(runtime: Handle) -> Self {
        Self {
            command_tasks: NativeTaskGroup::new(runtime),
            relay_lifecycle: RelayLifecycle::new(),
            relay_cancellation: CancellationToken::new(),
            relay: None,
        }
    }
}

fn run_event_loop_with_owner<F>(
    runtime: &Runtime,
    host: &mut CommandHost,
    options: NativeHostOptions,
    configure_ui: F,
    session: &mut NativeHostSession,
) -> Result<(), NativeHostError>
where
    F: FnOnce(&UiHandle),
{
    seed_missing_profiles(runtime, host, options.seed_profiles)?;

    let result = {
        let ui = UiHandle::new().map_err(|_| NativeHostError::Ui)?;
        ui.window()
            .set_workspace_label(options.workspace_label.into());
        configure_ui(&ui);

        let events = host.subscribe();
        let observer = host.lifecycle_observer();
        let weak_ui = ui.weak_handle();
        session.relay = Some(runtime.spawn(event_relay(
            events,
            observer,
            weak_ui,
            session.relay_lifecycle.clone(),
            session.relay_cancellation.clone(),
        )));
        if let Some(foreground) = options.inference_foreground.as_ref() {
            let foreground = foreground.clone();
            host.set_android_foreground_stop(Arc::new(move |generation| {
                foreground.stop_generation(generation)
            }))
            .map_err(|_| NativeHostError::Host)?;
        }
        connect_native_commands(
            host,
            &ui,
            &session.command_tasks,
            options.inference_foreground,
        );
        host.lifecycle_observer()
            .publish_ui_state()
            .map_err(|_| NativeHostError::Host)?;

        let show_result = ui.show().map_err(|_| NativeHostError::Ui);
        let event_loop_result = if show_result.is_ok() {
            slint::run_event_loop().map_err(|_| NativeHostError::EventLoop)
        } else {
            Ok(())
        };
        let hide_result = ui.hide().map_err(|_| NativeHostError::Ui);

        combine_ui_results(show_result, event_loop_result, hide_result)
    };

    result
}

fn seed_missing_profiles(
    runtime: &Runtime,
    host: &CommandHost,
    profiles: Vec<ProviderProfile>,
) -> Result<(), NativeHostError> {
    let mut known_ids = host
        .provider_profiles()
        .map_err(|_| NativeHostError::Host)?
        .into_iter()
        .map(|profile| profile.id)
        .collect::<HashSet<_>>();
    let client = host.client();
    for mut profile in profiles {
        if known_ids.contains(&profile.id) {
            continue;
        }
        profile.selected = false;
        let profile_id = profile.id.clone();
        runtime
            .block_on(client.dispatch(AppCommand::SaveProviderProfile { profile }))
            .map_err(|_| NativeHostError::Host)?;
        known_ids.insert(profile_id);
    }
    Ok(())
}

fn connect_native_commands(
    host: &CommandHost,
    ui: &UiHandle,
    commands: &NativeTaskGroup,
    inference_foreground: Option<Arc<dyn NativeInferenceForeground>>,
) {
    let client = host.client();
    let local = host.local_handle();
    let command_ui = ui.weak_handle();
    let command_tasks = commands.clone();

    connect_commands(ui.window(), move |command| {
        let client = client.clone();
        let local = local.clone();
        let command_ui = command_ui.clone();
        let command_for_failure = command.clone();
        let task_ui = command_ui.clone();
        let task_command_for_failure = command_for_failure.clone();
        let direct_local = is_direct_local_callback(&command);
        let admitted = command_tasks.spawn(async move {
            let dispatch = if direct_local {
                local.dispatch(command).await
            } else {
                client.dispatch(command).await
            };
            if dispatch.is_err() {
                let _ = deliver_ui_command_failure(task_ui, task_command_for_failure);
            }
        });

        if !admitted {
            let _ = deliver_ui_command_failure(command_ui, command_for_failure);
        }
    });

    let save_local = host.local_handle();
    let save_ui = ui.weak_handle();
    let save_commands = commands.clone();
    let clear_local = host.local_handle();
    let clear_ui = ui.weak_handle();
    let clear_commands = commands.clone();
    connect_provider_credentials(
        ui.window(),
        move |provider_id, mut credential_text, generation| {
            let credential = SecretValue::new(std::mem::take(&mut credential_text));
            credential_text.clear();
            let local = save_local.clone();
            let task_ui = save_ui.clone();
            let task_provider_id = provider_id.clone();
            let admitted = save_commands.spawn(async move {
                if save_local_provider_credential(&local, task_provider_id.clone(), credential)
                    .is_ok()
                {
                    let _ = deliver_ui_provider_credential_success(
                        task_ui,
                        task_provider_id,
                        "save",
                        generation,
                    );
                } else {
                    let _ = deliver_ui_provider_credential_failure(
                        task_ui,
                        task_provider_id,
                        "save",
                        generation,
                    );
                }
            });
            if !admitted {
                let _ = deliver_ui_provider_credential_failure(
                    save_ui.clone(),
                    provider_id,
                    "save",
                    generation,
                );
            }
        },
        move |provider_id, generation| {
            let local = clear_local.clone();
            let task_ui = clear_ui.clone();
            let task_provider_id = provider_id.clone();
            let admitted = clear_commands.spawn(async move {
                if clear_local_provider_credential(&local, task_provider_id.clone()).is_ok() {
                    let _ = deliver_ui_provider_credential_success(
                        task_ui,
                        task_provider_id,
                        "delete",
                        generation,
                    );
                } else {
                    let _ = deliver_ui_provider_credential_failure(
                        task_ui,
                        task_provider_id,
                        "delete",
                        generation,
                    );
                }
            });
            if !admitted {
                let _ = deliver_ui_provider_credential_failure(
                    clear_ui.clone(),
                    provider_id,
                    "delete",
                    generation,
                );
            }
        },
    );

    let inference = host.android_inference_control();
    ui.window()
        .set_local_inference_available(inference.is_some());
    if let Some(inference) = inference {
        ui.window().set_local_inference_detail(
            "Qwen3.5 0.8B Q4_0 is optional and runs only on this Android device.".into(),
        );
        let install_control = inference.clone();
        let install_ui = ui.weak_handle();
        let install_commands = commands.clone();
        let install_cancellation = commands.cancellation_token();
        let install_foreground = inference_foreground.clone();
        ui.window().on_install_and_load_local_model(move || {
            let control = install_control.clone();
            let task_ui = install_ui.clone();
            let cancellation = install_cancellation.child_token();
            let foreground = install_foreground.clone();
            let admitted = install_commands.spawn(async move {
                let operation = match control.begin_operation().await {
                    Ok(operation) => operation,
                    Err(_) => {
                        let _ = deliver_ui_local_inference_state(
                            task_ui,
                            false,
                            "Enable the local backend before loading a model.".into(),
                        );
                        return;
                    }
                };
                let generation = operation.foreground_generation();
                if foreground
                    .as_ref()
                    .is_some_and(|service| !service.start_generation(generation))
                {
                    let _ = deliver_ui_local_inference_state(
                        task_ui,
                        false,
                        "Android could not start the visible local-model service.".into(),
                    );
                    return;
                }
                match operation.install_and_activate_default(cancellation).await {
                    Ok(report) => {
                        let detail = format!(
                            "Qwen3.5 0.8B Q4_0 loaded through {} ({:?}).",
                            report.runtime(),
                            report.kind()
                        );
                        let _ = deliver_ui_local_inference_state(task_ui, true, detail);
                    }
                    Err(_) => {
                        if control.active_model().await.is_none() {
                            if let Some(service) = foreground {
                                let _ = service.stop_generation(generation);
                            }
                        }
                        let _ = deliver_ui_local_inference_state(
                            task_ui,
                            false,
                            "The on-device model could not be downloaded, verified, or loaded."
                                .into(),
                        );
                    }
                }
            });
            if !admitted {
                let _ = deliver_ui_local_inference_state(
                    install_ui.clone(),
                    false,
                    "The on-device model request was rejected during shutdown.".into(),
                );
            }
        });

        let unload_control = inference;
        let unload_ui = ui.weak_handle();
        let unload_commands = commands.clone();
        let unload_foreground = inference_foreground;
        ui.window().on_unload_local_model(move || {
            let control = unload_control.clone();
            let task_ui = unload_ui.clone();
            let foreground = unload_foreground.clone();
            let admitted = unload_commands.spawn(async move {
                let operation = match control.begin_operation().await {
                    Ok(operation) => operation,
                    Err(_) => {
                        let _ = deliver_ui_local_inference_state(
                            task_ui,
                            false,
                            "The local backend is disabled or changing.".into(),
                        );
                        return;
                    }
                };
                let generation = operation.foreground_generation();
                let result = operation.unload().await;
                if result.is_ok() {
                    if let Some(service) = foreground {
                        let _ = service.stop_generation(generation);
                    }
                }
                let detail = if result.is_ok() {
                    "The on-device model is unloaded."
                } else {
                    "The on-device model could not be unloaded cleanly."
                };
                let _ = deliver_ui_local_inference_state(task_ui, false, detail.into());
            });
            if !admitted {
                let _ = deliver_ui_local_inference_state(
                    unload_ui.clone(),
                    false,
                    "The unload request was rejected during shutdown.".into(),
                );
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeCredentialMutationError;

impl fmt::Display for NativeCredentialMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Argentum could not update the secure provider credential.")
    }
}

fn save_local_provider_credential(
    local: &LocalCommandHandle,
    provider_id: String,
    credential: SecretValue,
) -> Result<(), NativeCredentialMutationError> {
    local
        .set_provider_credential(&provider_id, credential)
        .map_err(|_| NativeCredentialMutationError)
}

fn clear_local_provider_credential(
    local: &LocalCommandHandle,
    provider_id: String,
) -> Result<(), NativeCredentialMutationError> {
    local
        .clear_provider_credential(&provider_id)
        .map_err(|_| NativeCredentialMutationError)
}

fn is_direct_local_callback(command: &AppCommand) -> bool {
    matches!(
        command,
        AppCommand::SelectExecutionProfile { .. }
            | AppCommand::SetHarnessCapabilityEnabled { .. }
            | AppCommand::ApproveTool { .. }
            | AppCommand::SetOnboardingPermission { .. }
    )
}

async fn event_relay(
    mut events: tokio::sync::broadcast::Receiver<AppEvent>,
    observer: CommandHostObserver,
    ui: WeakUiHandle,
    lifecycle: RelayLifecycle,
    cancellation: CancellationToken,
) -> Result<(), NativeHostError> {
    let mut pending_event = None;
    loop {
        tokio::select! {
            biased;
            event = next_coalesced_event(&mut events, &mut pending_event) => match event {
                CoalescedEvent::Event { event, lagged } => {
                    if !deliver_ui_event(ui.clone(), event) {
                        return handle_relay_failure(&lifecycle, || {
                            let _ = deliver_ui_relay_failure_and_quit(ui.clone());
                        });
                    }
                    if lagged.is_some() && !republish_after_lag(&observer, &ui) {
                        return handle_relay_failure(&lifecycle, || {
                            let _ = deliver_ui_relay_failure_and_quit(ui.clone());
                        });
                    }
                }
                CoalescedEvent::Lagged(skipped) => {
                    let _ = skipped;
                    if !republish_after_lag(&observer, &ui) {
                        return handle_relay_failure(&lifecycle, || {
                            let _ = deliver_ui_relay_failure_and_quit(ui.clone());
                        });
                    }
                }
                CoalescedEvent::Closed => return handle_relay_failure(&lifecycle, || {
                    let _ = deliver_ui_relay_failure_and_quit(ui.clone());
                }),
            },
            _ = cancellation.cancelled() => return Ok(()),
        }
    }
}

#[derive(Clone)]
struct RelayLifecycle {
    state: Arc<AtomicU8>,
}

impl RelayLifecycle {
    fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(RelayState::Active as u8)),
        }
    }

    fn begin_owner_shutdown(&self) -> bool {
        match self.state.compare_exchange(
            RelayState::Active as u8,
            RelayState::OwnerStopping as u8,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => true,
            Err(value) if value == RelayState::OwnerStopping as u8 => true,
            Err(value) if value == RelayState::Failed as u8 => false,
            Err(_) => false,
        }
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum RelayState {
    Active = 0,
    OwnerStopping = 1,
    Failed = 2,
}

fn handle_relay_failure<F>(
    lifecycle: &RelayLifecycle,
    report_and_quit: F,
) -> Result<(), NativeHostError>
where
    F: FnOnce(),
{
    match lifecycle.state.compare_exchange(
        RelayState::Active as u8,
        RelayState::Failed as u8,
        Ordering::SeqCst,
        Ordering::SeqCst,
    ) {
        Ok(_) => {
            report_and_quit();
            Err(NativeHostError::EventRelay)
        }
        Err(value) if value == RelayState::OwnerStopping as u8 => Ok(()),
        Err(value) if value == RelayState::Failed as u8 => Err(NativeHostError::EventRelay),
        Err(_) => Err(NativeHostError::EventRelay),
    }
}

async fn shutdown_native_owner(
    host: &mut impl NativeOwner,
    commands: &NativeTaskGroup,
    relay_lifecycle: RelayLifecycle,
    relay_cancellation: CancellationToken,
    relay: Option<JoinHandle<Result<(), NativeHostError>>>,
) -> Result<(), NativeHostError> {
    let mut failures = Vec::new();

    if host.request_shutdown().is_err() {
        failures.push(NativeCleanupStep::RequestShutdown);
    }
    commands.close();
    commands.wait().await;

    let _ = relay_lifecycle.begin_owner_shutdown();
    relay_cancellation.cancel();
    if let Some(relay) = relay {
        match relay.await {
            Ok(Ok(())) => {}
            Ok(Err(_)) | Err(_) => failures.push(NativeCleanupStep::EventRelay),
        }
    }

    shutdown_native_owner_until_complete(host).await;

    if failures.is_empty() {
        Ok(())
    } else {
        Err(NativeHostError::Cleanup(failures))
    }
}

async fn shutdown_native_owner_until_complete(host: &mut impl NativeOwner) {
    const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(25);
    const MAX_RETRY_DELAY: Duration = Duration::from_secs(1);

    let mut retry_delay = INITIAL_RETRY_DELAY;
    while host.shutdown().await.is_err() {
        tokio::time::sleep(retry_delay).await;
        retry_delay = retry_delay.saturating_mul(2).min(MAX_RETRY_DELAY);
    }
}

trait NativeOwner {
    type Error;

    fn request_shutdown(&mut self) -> Result<(), Self::Error>;

    async fn shutdown(&mut self) -> Result<(), Self::Error>;
}

impl NativeOwner for CommandHost {
    type Error = argentum_cli::HostError;

    fn request_shutdown(&mut self) -> Result<(), Self::Error> {
        CommandHost::request_shutdown(self)
    }

    async fn shutdown(&mut self) -> Result<(), Self::Error> {
        CommandHost::shutdown(self).await
    }
}

fn combine_ui_results(
    show: Result<(), NativeHostError>,
    event_loop: Result<(), NativeHostError>,
    hide: Result<(), NativeHostError>,
) -> Result<(), NativeHostError> {
    show.and(event_loop).and(hide)
}

fn combine_application_and_cleanup(
    application: Result<(), NativeHostError>,
    cleanup: Result<(), NativeHostError>,
) -> Result<(), NativeHostError> {
    match (application, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(application), Ok(())) => Err(application),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(application), Err(cleanup)) => Err(NativeHostError::ApplicationAndCleanup {
            application: Box::new(application),
            cleanup: Box::new(cleanup),
        }),
    }
}

#[derive(Clone)]
struct NativeTaskGroup {
    state: Arc<Mutex<NativeTaskGroupState>>,
    runtime: Handle,
    cancellation: CancellationToken,
}

struct NativeTaskGroupState {
    accepting: bool,
    tracker: TaskTracker,
}

impl NativeTaskGroup {
    fn new(runtime: Handle) -> Self {
        Self {
            state: Arc::new(Mutex::new(NativeTaskGroupState {
                accepting: true,
                tracker: TaskTracker::new(),
            })),
            runtime,
            cancellation: CancellationToken::new(),
        }
    }

    fn spawn<F>(&self, task: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.accepting {
            return false;
        }
        state.tracker.spawn_on(task, &self.runtime);
        true
    }

    fn close(&self) {
        self.cancellation.cancel();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.accepting = false;
        state.tracker.close();
    }

    fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    async fn wait(&self) {
        let tracker = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .tracker
            .clone();
        tracker.wait().await;
    }
}
