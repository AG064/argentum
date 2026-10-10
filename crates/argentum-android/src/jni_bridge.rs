#[cfg(any(test, target_os = "android"))]
use std::collections::HashMap;
#[cfg(any(test, target_os = "android"))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
#[cfg(any(test, target_os = "android"))]
use std::sync::{Arc, OnceLock};

pub const LIFECYCLE_PROTOCOL_VERSION: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum LifecycleState {
    Created = 1,
    Started = 2,
    Resumed = 3,
    Paused = 4,
    Stopped = 5,
    Destroyed = 6,
}

impl LifecycleState {
    fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::Created),
            2 => Some(Self::Started),
            3 => Some(Self::Resumed),
            4 => Some(Self::Paused),
            5 => Some(Self::Stopped),
            6 => Some(Self::Destroyed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LifecycleSnapshot {
    pub activity_generation: u64,
    pub sequence: u64,
    pub state: Option<LifecycleState>,
}

#[derive(Debug, Default)]
pub struct LifecycleProjection {
    accepted: Mutex<LifecycleSnapshot>,
}

impl LifecycleProjection {
    pub fn accept(
        &self,
        protocol: i32,
        activity_generation: u64,
        sequence: u64,
        state: LifecycleState,
    ) -> bool {
        self.accept_message(protocol, activity_generation, sequence, Some(state))
    }

    pub fn accept_code(
        &self,
        protocol: i32,
        activity_generation: u64,
        sequence: u64,
        state_code: i32,
    ) -> bool {
        let Some(state) = LifecycleState::from_code(state_code) else {
            return false;
        };
        self.accept(protocol, activity_generation, sequence, state)
    }

    pub fn accept_back(&self, protocol: i32, activity_generation: u64, sequence: u64) -> bool {
        self.accept_message(protocol, activity_generation, sequence, None)
    }

    pub fn snapshot(&self) -> LifecycleSnapshot {
        *self.accepted.lock().expect("lifecycle projection lock")
    }

    fn accept_message(
        &self,
        protocol: i32,
        activity_generation: u64,
        sequence: u64,
        state: Option<LifecycleState>,
    ) -> bool {
        if protocol != LIFECYCLE_PROTOCOL_VERSION || activity_generation == 0 || sequence == 0 {
            return false;
        }
        let mut accepted = self.accepted.lock().expect("lifecycle projection lock");
        if activity_generation < accepted.activity_generation {
            return false;
        }
        if activity_generation > accepted.activity_generation {
            if state != Some(LifecycleState::Created) {
                return false;
            }
            accepted.activity_generation = activity_generation;
            accepted.sequence = 0;
            accepted.state = None;
        }
        if sequence <= accepted.sequence {
            return false;
        }
        accepted.sequence = sequence;
        if let Some(state) = state {
            accepted.state = Some(state);
        }
        true
    }
}

#[cfg(any(test, target_os = "android"))]
type CloseAction = dyn Fn() -> bool + Send + Sync + 'static;

#[cfg(any(test, target_os = "android"))]
#[derive(Default)]
struct CloseRegistry {
    generation: AtomicU64,
    current: Mutex<HashMap<u64, (u64, Arc<CloseAction>)>>,
}

#[cfg(any(test, target_os = "android"))]
pub(crate) struct CloseActionRegistration {
    activity_generation: u64,
    registration_generation: u64,
}

#[cfg(any(test, target_os = "android"))]
impl Drop for CloseActionRegistration {
    fn drop(&mut self) {
        let registry = close_registry();
        let mut current = registry.current.lock().expect("close action lock");
        if current
            .get(&self.activity_generation)
            .is_some_and(|(registration_generation, _)| {
                *registration_generation == self.registration_generation
            })
        {
            current.remove(&self.activity_generation);
        }
    }
}

#[cfg(any(test, target_os = "android"))]
pub(crate) fn register_close_action(
    activity_generation: u64,
    action: impl Fn() -> bool + Send + Sync + 'static,
) -> CloseActionRegistration {
    let registry = close_registry();
    let registration_generation = registry.generation.fetch_add(1, Ordering::Relaxed) + 1;
    registry.current.lock().expect("close action lock").insert(
        activity_generation,
        (registration_generation, Arc::new(action)),
    );
    CloseActionRegistration {
        activity_generation,
        registration_generation,
    }
}

#[cfg(any(test, target_os = "android"))]
fn close_registry() -> &'static CloseRegistry {
    static REGISTRY: OnceLock<CloseRegistry> = OnceLock::new();
    REGISTRY.get_or_init(CloseRegistry::default)
}

#[cfg(target_os = "android")]
fn process_lifecycle() -> &'static LifecycleProjection {
    static PROJECTION: OnceLock<LifecycleProjection> = OnceLock::new();
    PROJECTION.get_or_init(LifecycleProjection::default)
}

#[cfg(any(test, target_os = "android"))]
fn request_registered_close(activity_generation: u64) -> bool {
    let action = close_registry()
        .current
        .lock()
        .expect("close action lock")
        .get(&activity_generation)
        .map(|(_, action)| action.clone());
    action.is_some_and(|action| action())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_argentum_ArgentumActivity_nativeOnLifecycle(
    _env: *mut jni::sys::JNIEnv,
    _activity: jni::sys::jobject,
    state: jni::sys::jint,
    activity_generation: jni::sys::jlong,
    sequence: jni::sys::jlong,
    protocol: jni::sys::jint,
) {
    let Ok(activity_generation) = u64::try_from(activity_generation) else {
        return;
    };
    let Ok(sequence) = u64::try_from(sequence) else {
        return;
    };
    let _ = process_lifecycle().accept_code(protocol, activity_generation, sequence, state);
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_argentum_ArgentumActivity_nativeOnBackRequested(
    _env: *mut jni::sys::JNIEnv,
    _activity: jni::sys::jobject,
    activity_generation: jni::sys::jlong,
    sequence: jni::sys::jlong,
    protocol: jni::sys::jint,
) -> jni::sys::jboolean {
    let accepted = u64::try_from(activity_generation)
        .ok()
        .is_some_and(|activity_generation| {
            u64::try_from(sequence).ok().is_some_and(|sequence| {
                process_lifecycle().accept_back(protocol, activity_generation, sequence)
            })
        });
    if accepted && request_registered_close(activity_generation as u64) {
        jni::sys::JNI_TRUE
    } else {
        jni::sys::JNI_FALSE
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

    use super::{
        register_close_action, request_registered_close, LifecycleProjection, LifecycleState,
        LIFECYCLE_PROTOCOL_VERSION,
    };

    fn close_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("close test lock")
    }

    #[test]
    fn lifecycle_bridge_rejects_stale_or_wrong_protocol_messages() {
        let state = LifecycleProjection::default();
        assert!(state.accept(LIFECYCLE_PROTOCOL_VERSION, 1, 1, LifecycleState::Created));
        assert!(!state.accept(1, 1, 2, LifecycleState::Resumed));
        assert!(!state.accept(LIFECYCLE_PROTOCOL_VERSION, 1, 1, LifecycleState::Resumed));
        assert_eq!(state.snapshot().state, Some(LifecycleState::Created));
        assert_eq!(state.snapshot().sequence, 1);
    }

    #[test]
    fn lifecycle_bridge_rejects_zero_and_unknown_states_without_mutation() {
        let state = LifecycleProjection::default();
        assert!(!state.accept_code(LIFECYCLE_PROTOCOL_VERSION, 0, 1, 1));
        assert!(!state.accept_code(LIFECYCLE_PROTOCOL_VERSION, 1, 0, 1));
        assert!(!state.accept_code(LIFECYCLE_PROTOCOL_VERSION, 1, 1, 0));
        assert!(!state.accept_code(LIFECYCLE_PROTOCOL_VERSION, 1, 1, 7));
        assert_eq!(state.snapshot().activity_generation, 0);
        assert_eq!(state.snapshot().sequence, 0);
        assert_eq!(state.snapshot().state, None);
    }

    #[test]
    fn back_advances_sequence_without_changing_lifecycle_state() {
        let state = LifecycleProjection::default();
        assert!(state.accept(LIFECYCLE_PROTOCOL_VERSION, 1, 1, LifecycleState::Created));
        assert!(state.accept(LIFECYCLE_PROTOCOL_VERSION, 1, 2, LifecycleState::Resumed));
        assert!(state.accept_back(LIFECYCLE_PROTOCOL_VERSION, 1, 3));
        assert_eq!(state.snapshot().sequence, 3);
        assert_eq!(state.snapshot().state, Some(LifecycleState::Resumed));
        assert!(!state.accept(LIFECYCLE_PROTOCOL_VERSION, 1, 3, LifecycleState::Paused));
    }

    #[test]
    fn newer_activity_generation_rejects_late_messages_from_the_retired_instance() {
        let state = LifecycleProjection::default();
        assert!(state.accept(LIFECYCLE_PROTOCOL_VERSION, 4, 1, LifecycleState::Created));
        assert!(state.accept(LIFECYCLE_PROTOCOL_VERSION, 4, 2, LifecycleState::Resumed));
        assert!(state.accept(LIFECYCLE_PROTOCOL_VERSION, 5, 1, LifecycleState::Created));
        assert!(!state.accept(LIFECYCLE_PROTOCOL_VERSION, 4, 3, LifecycleState::Destroyed));
        assert!(!state.accept_back(LIFECYCLE_PROTOCOL_VERSION, 4, 4));
        assert!(state.accept(LIFECYCLE_PROTOCOL_VERSION, 5, 2, LifecycleState::Resumed));
        assert_eq!(state.snapshot().activity_generation, 5);
        assert_eq!(state.snapshot().state, Some(LifecycleState::Resumed));
    }

    #[test]
    fn back_is_unhandled_without_a_registered_trusted_close_action() {
        let _guard = close_test_lock();
        assert!(!request_registered_close(41));
        let calls = Arc::new(AtomicUsize::new(0));
        {
            let action_calls = calls.clone();
            let registration = register_close_action(41, move || {
                action_calls.fetch_add(1, Ordering::Relaxed);
                true
            });
            assert!(request_registered_close(41));
            assert!(!request_registered_close(42));
            drop(registration);
        }
        assert!(!request_registered_close(41));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn registered_close_action_controls_back_handling() {
        let _guard = close_test_lock();
        let registration = register_close_action(51, || false);
        assert!(!request_registered_close(51));
        drop(registration);
    }

    #[test]
    fn close_actions_are_isolated_by_activity_generation() {
        let _guard = close_test_lock();
        let first_calls = Arc::new(AtomicUsize::new(0));
        let second_calls = Arc::new(AtomicUsize::new(0));
        let first_action_calls = first_calls.clone();
        let second_action_calls = second_calls.clone();
        let first = register_close_action(61, move || {
            first_action_calls.fetch_add(1, Ordering::Relaxed);
            true
        });
        let second = register_close_action(62, move || {
            second_action_calls.fetch_add(1, Ordering::Relaxed);
            true
        });

        assert!(request_registered_close(61));
        assert!(request_registered_close(62));
        assert_eq!(first_calls.load(Ordering::Relaxed), 1);
        assert_eq!(second_calls.load(Ordering::Relaxed), 1);

        drop(first);
        assert!(!request_registered_close(61));
        assert!(request_registered_close(62));
        drop(second);
    }
}
