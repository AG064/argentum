use argentum_cli::CommandHostObserver;
use argentum_domain::{AppCommand, AppEvent};
use argentum_ui::WeakUiHandle;
use tokio::sync::broadcast::{error::RecvError, error::TryRecvError, Receiver};

pub(crate) struct CoalescedAppEvent(Box<AppEvent>);

impl From<AppEvent> for CoalescedAppEvent {
    fn from(event: AppEvent) -> Self {
        Self(Box::new(event))
    }
}

impl From<CoalescedAppEvent> for AppEvent {
    fn from(event: CoalescedAppEvent) -> Self {
        *event.0
    }
}

pub(crate) enum CoalescedEvent {
    Event {
        event: CoalescedAppEvent,
        lagged: Option<u64>,
    },
    Lagged(u64),
    Closed,
}

pub(crate) async fn next_coalesced_event(
    events: &mut Receiver<AppEvent>,
    pending_event: &mut Option<AppEvent>,
) -> CoalescedEvent {
    let received = match pending_event.take() {
        Some(event) => Ok(event),
        None => events.recv().await,
    };

    match received {
        Ok(
            mut
            event @ (AppEvent::AssistantDelta { .. } | AppEvent::AssistantReasoningDelta { .. }),
        ) => {
            let mut lagged = None;
            loop {
                match events.try_recv() {
                    Ok(next_event) => {
                        if let Some(next_event) =
                            append_matching_stream_delta(&mut event, next_event)
                        {
                            *pending_event = Some(next_event);
                            break;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Lagged(skipped)) => {
                        lagged = Some(skipped);
                        break;
                    }
                    Err(TryRecvError::Closed) => break,
                }
            }
            CoalescedEvent::Event {
                event: event.into(),
                lagged,
            }
        }
        Ok(event) => CoalescedEvent::Event {
            event: event.into(),
            lagged: None,
        },
        Err(RecvError::Lagged(skipped)) => CoalescedEvent::Lagged(skipped),
        Err(RecvError::Closed) => CoalescedEvent::Closed,
    }
}

pub(crate) fn deliver_ui_event(ui: WeakUiHandle, event: impl Into<AppEvent>) -> bool {
    let event = event.into();
    slint::invoke_from_event_loop(move || {
        if let Some(handle) = ui.upgrade() {
            handle.apply_event(&event);
        }
    })
    .is_ok()
}

pub(crate) fn deliver_ui_command_failure(ui: WeakUiHandle, command: AppCommand) -> bool {
    slint::invoke_from_event_loop(move || {
        if let Some(handle) = ui.upgrade() {
            handle.apply_command_failure(
                &command,
                "Argentum could not complete that command. Review the current workspace settings and try again.",
            );
        }
    })
    .is_ok()
}

pub(crate) fn deliver_ui_provider_credential_failure(
    ui: WeakUiHandle,
    provider_id: String,
    action: &'static str,
    generation: i32,
) -> bool {
    slint::invoke_from_event_loop(move || {
        if let Some(handle) = ui.upgrade() {
            handle.apply_provider_credential_failure(
                &provider_id,
                action,
                generation,
                "Argentum could not update the secure provider credential. Try again.",
            );
        }
    })
    .is_ok()
}

pub(crate) fn deliver_ui_provider_credential_success(
    ui: WeakUiHandle,
    provider_id: String,
    action: &'static str,
    generation: i32,
) -> bool {
    slint::invoke_from_event_loop(move || {
        if let Some(handle) = ui.upgrade() {
            handle.apply_provider_credential_success(&provider_id, action, generation);
        }
    })
    .is_ok()
}

pub(crate) fn deliver_ui_local_inference_state(
    ui: WeakUiHandle,
    active: bool,
    detail: String,
) -> bool {
    slint::invoke_from_event_loop(move || {
        if let Some(handle) = ui.upgrade() {
            handle.window().set_local_inference_active(active);
            handle.window().set_local_inference_busy(false);
            handle.window().set_local_inference_detail(detail.into());
        }
    })
    .is_ok()
}

pub(crate) fn deliver_ui_relay_failure_and_quit(ui: WeakUiHandle) -> bool {
    let error_queued = deliver_ui_event(
        ui,
        AppEvent::Error {
            message: "Argentum could not keep the native interface synchronized.".into(),
            recoverable: false,
        },
    );
    let quit_requested = slint::quit_event_loop().is_ok();
    error_queued && quit_requested
}

pub(crate) fn republish_after_lag(observer: &CommandHostObserver, ui: &WeakUiHandle) -> bool {
    if observer.publish_ui_state().is_ok() {
        return true;
    }

    deliver_ui_event(
        ui.clone(),
        AppEvent::Error {
            message: "Argentum could not refresh the current workspace state.".into(),
            recoverable: true,
        },
    )
}

pub fn append_matching_stream_delta(current: &mut AppEvent, next: AppEvent) -> Option<AppEvent> {
    match (current, next) {
        (
            AppEvent::AssistantDelta { run_id, text },
            AppEvent::AssistantDelta {
                run_id: next_run_id,
                text: next_text,
            },
        ) if *run_id == next_run_id => {
            text.push_str(&next_text);
            None
        }
        (
            AppEvent::AssistantReasoningDelta { run_id, text },
            AppEvent::AssistantReasoningDelta {
                run_id: next_run_id,
                text: next_text,
            },
        ) if *run_id == next_run_id => {
            text.push_str(&next_text);
            None
        }
        (_, next) => Some(next),
    }
}
