use std::fmt;
use std::sync::Arc;

use argentum_security::SecretValue;
use zeroize::Zeroize;

use crate::{PlatformError, SecretStore};

const MAX_SECRET_BYTES: usize = 16 * 1024;
const SECURE_STORAGE_FAILURE: &str = "Android secure storage operation failed";
const ACCEPTED_KEYS: [&str; 3] = ["provider/openai", "provider/minimax", "provider/deepseek"];

trait SecretBridge: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, PlatformError>;
    fn set(&self, key: &str, value: &[u8]) -> Result<(), PlatformError>;
    fn delete(&self, key: &str) -> Result<(), PlatformError>;
}

#[cfg(any(target_os = "android", test))]
trait SecretArrayCleanup {
    fn exception_pending(&mut self) -> bool;
    fn clear_exception(&mut self);
    fn clear_array(&mut self) -> Result<(), ()>;
}

#[cfg(any(target_os = "android", test))]
fn prepare_secret_array_operation(
    cleanup: &mut impl SecretArrayCleanup,
) -> Result<(), PlatformError> {
    if cleanup.exception_pending() {
        cleanup.clear_exception();
        let _ = cleanup.clear_array();
        return Err(fixed_failure());
    }
    Ok(())
}

#[cfg(any(target_os = "android", test))]
fn finish_secret_array_operation<T: Zeroize>(
    cleanup: &mut impl SecretArrayCleanup,
    operation: Result<T, ()>,
) -> Result<T, PlatformError> {
    match operation {
        Ok(mut value) => match cleanup.clear_array() {
            Ok(()) => Ok(value),
            Err(()) => {
                value.zeroize();
                if cleanup.exception_pending() {
                    cleanup.clear_exception();
                    let _ = cleanup.clear_array();
                }
                Err(fixed_failure())
            }
        },
        Err(()) => {
            if cleanup.exception_pending() {
                cleanup.clear_exception();
            }
            let _ = cleanup.clear_array();
            Err(fixed_failure())
        }
    }
}

#[cfg(test)]
fn run_secret_array_operation<T: Zeroize>(
    cleanup: &mut impl SecretArrayCleanup,
    operation: impl FnOnce() -> Result<T, ()>,
) -> Result<T, PlatformError> {
    prepare_secret_array_operation(cleanup)?;
    let result = operation();
    finish_secret_array_operation(cleanup, result)
}

pub struct AndroidSecretStore {
    bridge: Arc<dyn SecretBridge>,
}

impl fmt::Debug for AndroidSecretStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AndroidSecretStore")
            .field("bridge", &"[REDACTED]")
            .finish()
    }
}

impl AndroidSecretStore {
    #[cfg(target_os = "android")]
    pub fn new(app: android_activity::AndroidApp) -> Self {
        Self {
            bridge: Arc::new(JniSecretBridge { app }),
        }
    }

    #[cfg(test)]
    fn from_bridge(bridge: Arc<dyn SecretBridge>) -> Self {
        Self { bridge }
    }

    fn validate_key(key: &str) -> Result<(), PlatformError> {
        if ACCEPTED_KEYS.contains(&key) {
            Ok(())
        } else {
            Err(fixed_failure())
        }
    }
}

impl SecretStore for AndroidSecretStore {
    fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
        Self::validate_key(key)?;
        let Some(mut bytes) = self.bridge.get(key)? else {
            return Ok(None);
        };
        if bytes.is_empty() || bytes.len() > MAX_SECRET_BYTES {
            bytes.zeroize();
            return Err(fixed_failure());
        }
        match String::from_utf8(bytes) {
            Ok(value) if secret_text_is_valid(&value) => Ok(Some(SecretValue::new(value))),
            Ok(mut value) => {
                value.zeroize();
                Err(fixed_failure())
            }
            Err(error) => {
                let mut bytes = error.into_bytes();
                bytes.zeroize();
                Err(fixed_failure())
            }
        }
    }

    fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
        Self::validate_key(key)?;
        let bytes = value.expose().as_bytes();
        if !secret_text_is_valid(value.expose()) {
            return Err(fixed_failure());
        }
        self.bridge.set(key, bytes)
    }

    fn delete(&self, key: &str) -> Result<(), PlatformError> {
        Self::validate_key(key)?;
        self.bridge.delete(key)
    }
}

fn fixed_failure() -> PlatformError {
    PlatformError::SecureStorage(SECURE_STORAGE_FAILURE.to_owned())
}

fn secret_text_is_valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_SECRET_BYTES && !value.chars().any(char::is_control)
}

#[cfg(target_os = "android")]
fn clear_java_byte_array(
    env: &jni::Env<'_>,
    array: &jni::objects::JByteArray<'_>,
    length: usize,
) -> jni::errors::Result<()> {
    const CLEAR_CHUNK_BYTES: usize = 4096;
    let zeros = [0_i8; CLEAR_CHUNK_BYTES];
    let mut offset = 0_usize;
    while offset < length {
        let count = (length - offset).min(CLEAR_CHUNK_BYTES);
        array.set_region(env, offset as jni::sys::jsize, &zeros[..count])?;
        offset += count;
    }
    Ok(())
}

#[cfg(target_os = "android")]
struct JniSecretArrayCleanup<'array, 'env> {
    env: &'env jni::Env<'env>,
    array: &'array jni::objects::JByteArray<'env>,
    length: usize,
}

#[cfg(target_os = "android")]
impl SecretArrayCleanup for JniSecretArrayCleanup<'_, '_> {
    fn exception_pending(&mut self) -> bool {
        self.env.exception_check()
    }

    fn clear_exception(&mut self) {
        self.env.exception_clear();
    }

    fn clear_array(&mut self) -> Result<(), ()> {
        clear_java_byte_array(self.env, self.array, self.length).map_err(|_| ())
    }
}

#[cfg(target_os = "android")]
fn clear_pending_jni_exception(env: &jni::Env<'_>) {
    if env.exception_check() {
        env.exception_clear();
    }
}

#[cfg(target_os = "android")]
fn fixed_jni_result<T>(
    env: &jni::Env<'_>,
    result: jni::errors::Result<T>,
) -> Result<T, PlatformError> {
    result.map_err(|_| {
        clear_pending_jni_exception(env);
        fixed_failure()
    })
}

#[cfg(target_os = "android")]
struct JniSecretBridge {
    app: android_activity::AndroidApp,
}

#[cfg(target_os = "android")]
impl JniSecretBridge {
    fn with_env<T>(
        &self,
        callback: impl FnOnce(&mut jni::Env<'_>, &jni::objects::JObject<'_>) -> Result<T, PlatformError>,
    ) -> Result<T, PlatformError> {
        let vm = unsafe { jni::JavaVM::from_raw(self.app.vm_as_ptr().cast()) };
        let raw_activity = self.app.activity_as_ptr() as jni::sys::jobject;
        vm.attach_current_thread(|env| -> jni::errors::Result<Result<T, PlatformError>> {
            let activity = unsafe {
                env.as_cast_raw::<jni::refs::Global<jni::objects::JObject<'_>>>(&raw_activity)?
            };
            Ok(callback(env, activity.as_ref()))
        })
        .map_err(|_| fixed_failure())
        .and_then(|result| result)
    }
}

#[cfg(target_os = "android")]
impl SecretBridge for JniSecretBridge {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, PlatformError> {
        self.with_env(|env, activity| {
            use jni::objects::{JByteArray, JValue};
            use jni::{jni_sig, jni_str};

            let java_key_result = env.new_string(key);
            let java_key = fixed_jni_result(env, java_key_result)?;
            let call = env.call_method(
                activity,
                jni_str!("argentumSecretGet"),
                jni_sig!("(Ljava/lang/String;)[B"),
                &[JValue::Object(java_key.as_ref())],
            );
            let value = fixed_jni_result(env, call)?;
            let object = fixed_jni_result(env, value.into_object())?;
            if object.is_null() {
                return Ok(None);
            }
            let array_result = JByteArray::cast_local(env, object);
            let array = fixed_jni_result(env, array_result)?;
            let length = fixed_jni_result(env, array.len(env))?;
            {
                let mut cleanup = JniSecretArrayCleanup {
                    env,
                    array: &array,
                    length,
                };
                prepare_secret_array_operation(&mut cleanup)?;
            }
            let converted = if length > MAX_SECRET_BYTES {
                Err(())
            } else {
                env.convert_byte_array(&array).map_err(|_| ())
            };
            let mut cleanup = JniSecretArrayCleanup {
                env,
                array: &array,
                length,
            };
            match finish_secret_array_operation(&mut cleanup, converted) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) => Err(error),
            }
        })
    }

    fn set(&self, key: &str, value: &[u8]) -> Result<(), PlatformError> {
        self.with_env(|env, activity| {
            use jni::objects::JValue;
            use jni::{jni_sig, jni_str};

            let java_key_result = env.new_string(key);
            let java_key = fixed_jni_result(env, java_key_result)?;
            let java_value_result = env.byte_array_from_slice(value);
            let java_value = fixed_jni_result(env, java_value_result)?;
            {
                let mut cleanup = JniSecretArrayCleanup {
                    env,
                    array: &java_value,
                    length: value.len(),
                };
                prepare_secret_array_operation(&mut cleanup)?;
            }
            let result = env
                .call_method(
                    activity,
                    jni_str!("argentumSecretSet"),
                    jni_sig!("(Ljava/lang/String;[B)V"),
                    &[
                        JValue::Object(java_key.as_ref()),
                        JValue::Object(java_value.as_ref()),
                    ],
                )
                .and_then(|returned| returned.v())
                .map_err(|_| ());
            let mut cleanup = JniSecretArrayCleanup {
                env,
                array: &java_value,
                length: value.len(),
            };
            finish_secret_array_operation(&mut cleanup, result)
        })
    }

    fn delete(&self, key: &str) -> Result<(), PlatformError> {
        self.with_env(|env, activity| {
            use jni::objects::JValue;
            use jni::{jni_sig, jni_str};

            let java_key_result = env.new_string(key);
            let java_key = fixed_jni_result(env, java_key_result)?;
            let call = env.call_method(
                activity,
                jni_str!("argentumSecretDelete"),
                jni_sig!("(Ljava/lang/String;)V"),
                &[JValue::Object(java_key.as_ref())],
            );
            let returned = fixed_jni_result(env, call)?;
            fixed_jni_result(env, returned.v())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use argentum_security::SecretValue;

    use super::{
        run_secret_array_operation, AndroidSecretStore, PlatformError, SecretArrayCleanup,
        SecretBridge,
    };
    use crate::SecretStore;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Call {
        method: &'static str,
        key: String,
        byte_count: usize,
    }

    #[derive(Debug, Default)]
    struct RecordingSecretBridge {
        calls: Mutex<Vec<Call>>,
        response: Mutex<Response>,
    }

    #[derive(Debug, Default)]
    enum Response {
        #[default]
        None,
        Empty,
        InvalidUtf8,
        Newline,
        Nul,
        MixedControl,
        Bounded,
        Oversized,
    }

    impl RecordingSecretBridge {
        fn calls(&self) -> Vec<Call> {
            self.calls.lock().expect("calls lock").clone()
        }

        fn call_count(&self) -> usize {
            self.calls.lock().expect("calls lock").len()
        }

        fn with_response(response: Response) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                response: Mutex::new(response),
            }
        }
    }

    impl SecretBridge for RecordingSecretBridge {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, PlatformError> {
            self.calls.lock().expect("calls lock").push(Call {
                method: "get",
                key: key.to_owned(),
                byte_count: 0,
            });
            Ok(match *self.response.lock().expect("response lock") {
                Response::None => None,
                Response::Empty => Some(Vec::new()),
                Response::InvalidUtf8 => Some(vec![0xff]),
                Response::Newline => Some(b"line\nbreak".to_vec()),
                Response::Nul => Some(b"nul\0inside".to_vec()),
                Response::MixedControl => Some("safe\u{0085}blocked".as_bytes().to_vec()),
                Response::Bounded => Some(vec![b'x'; 16 * 1024]),
                Response::Oversized => Some(vec![b'x'; 16 * 1024 + 1]),
            })
        }

        fn set(&self, key: &str, value: &[u8]) -> Result<(), PlatformError> {
            self.calls.lock().expect("calls lock").push(Call {
                method: "set",
                key: key.to_owned(),
                byte_count: value.len(),
            });
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.calls.lock().expect("calls lock").push(Call {
                method: "delete",
                key: key.to_owned(),
                byte_count: 0,
            });
            Ok(())
        }
    }

    #[test]
    fn android_secret_store_accepts_only_canonical_provider_keys() {
        let bridge = Arc::new(RecordingSecretBridge::default());
        let store = AndroidSecretStore::from_bridge(bridge.clone());

        for key in ["provider/openai", "provider/minimax", "provider/deepseek"] {
            store
                .set(key, SecretValue::new("bounded-secret"))
                .expect("canonical key");
            store.get(key).expect("canonical key");
            store.delete(key).expect("canonical key");
        }

        assert_eq!(bridge.call_count(), 9);
        assert!(bridge
            .calls()
            .iter()
            .all(|call| call.byte_count == 0 || call.byte_count == 14));
    }

    #[test]
    fn android_secret_store_rejects_noncanonical_keys_before_the_bridge() {
        let bridge = Arc::new(RecordingSecretBridge::default());
        let store = AndroidSecretStore::from_bridge(bridge.clone());

        for key in [
            "../deepseek",
            "provider/DeepSeek",
            " provider/deepseek",
            "provider/deepseek ",
            "provider/openai/extra",
            "provider/unknown",
            "",
        ] {
            assert!(store.set(key, SecretValue::new("blocked")).is_err());
            assert!(store.get(key).is_err());
            assert!(store.delete(key).is_err());
        }
        assert_eq!(bridge.call_count(), 0);
    }

    #[test]
    fn android_secret_store_enforces_the_secret_byte_bound() {
        let bridge = Arc::new(RecordingSecretBridge::default());
        let store = AndroidSecretStore::from_bridge(bridge.clone());
        let bounded = "x".repeat(16 * 1024);
        let oversized = "x".repeat(16 * 1024 + 1);

        assert!(store
            .set("provider/deepseek", SecretValue::new(oversized))
            .is_err());
        assert_eq!(bridge.call_count(), 0);
        store
            .set("provider/deepseek", SecretValue::new(bounded))
            .expect("bounded secret");
        assert_eq!(bridge.call_count(), 1);
        assert_eq!(bridge.calls()[0].byte_count, 16 * 1024);
    }

    #[test]
    fn android_secret_store_rejects_empty_and_control_text_before_the_bridge() {
        let bridge = Arc::new(RecordingSecretBridge::default());
        let store = AndroidSecretStore::from_bridge(bridge.clone());

        for invalid in ["", "nul\0inside", "line\nbreak", "safe\u{0085}blocked"] {
            assert!(store
                .set("provider/deepseek", SecretValue::new(invalid))
                .is_err());
        }
        assert_eq!(bridge.call_count(), 0);
    }

    #[test]
    fn android_secret_store_rejects_invalid_retrieved_plaintext() {
        for response in [
            Response::Empty,
            Response::InvalidUtf8,
            Response::Newline,
            Response::Nul,
            Response::MixedControl,
        ] {
            let bridge = Arc::new(RecordingSecretBridge::with_response(response));
            let store = AndroidSecretStore::from_bridge(bridge.clone());
            assert!(store.get("provider/deepseek").is_err());
            assert_eq!(bridge.call_count(), 1);
        }
    }

    #[test]
    fn android_secret_store_enforces_retrieved_secret_byte_bound() {
        let bounded_bridge = Arc::new(RecordingSecretBridge::with_response(Response::Bounded));
        let bounded_store = AndroidSecretStore::from_bridge(bounded_bridge);
        let bounded = bounded_store
            .get("provider/deepseek")
            .expect("bounded retrieval")
            .expect("configured retrieval");
        assert_eq!(bounded.expose().len(), 16 * 1024);

        let oversized_bridge = Arc::new(RecordingSecretBridge::with_response(Response::Oversized));
        let oversized_store = AndroidSecretStore::from_bridge(oversized_bridge);
        assert!(oversized_store.get("provider/deepseek").is_err());
    }

    #[derive(Debug)]
    struct RecordingCleanup {
        pending: VecDeque<bool>,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl SecretArrayCleanup for RecordingCleanup {
        fn exception_pending(&mut self) -> bool {
            self.events.lock().expect("events lock").push("check");
            self.pending.pop_front().unwrap_or(false)
        }

        fn clear_exception(&mut self) {
            self.events
                .lock()
                .expect("events lock")
                .push("exception-clear");
        }

        fn clear_array(&mut self) -> Result<(), ()> {
            self.events.lock().expect("events lock").push("array-clear");
            Ok(())
        }
    }

    #[test]
    fn pending_exception_is_cleared_before_array_and_method_is_suppressed() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let operation_events = events.clone();
        let mut cleanup = RecordingCleanup {
            pending: VecDeque::from([true]),
            events: events.clone(),
        };

        assert!(run_secret_array_operation(&mut cleanup, || {
            operation_events
                .lock()
                .expect("events lock")
                .push("operation");
            Ok(())
        })
        .is_err());
        assert_eq!(
            *events.lock().expect("events lock"),
            ["check", "exception-clear", "array-clear"]
        );
    }

    #[test]
    fn conversion_failure_clears_exception_before_secret_array() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let operation_events = events.clone();
        let mut cleanup = RecordingCleanup {
            pending: VecDeque::from([false, true]),
            events: events.clone(),
        };

        assert!(run_secret_array_operation(&mut cleanup, || {
            operation_events
                .lock()
                .expect("events lock")
                .push("operation");
            Err::<(), ()>(())
        })
        .is_err());
        assert_eq!(
            *events.lock().expect("events lock"),
            [
                "check",
                "operation",
                "check",
                "exception-clear",
                "array-clear"
            ]
        );
    }

    #[test]
    fn successful_array_operation_still_clears_secret_array() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let operation_events = events.clone();
        let mut cleanup = RecordingCleanup {
            pending: VecDeque::from([false]),
            events: events.clone(),
        };

        run_secret_array_operation(&mut cleanup, || {
            operation_events
                .lock()
                .expect("events lock")
                .push("operation");
            Ok(())
        })
        .expect("successful operation");
        assert_eq!(
            *events.lock().expect("events lock"),
            ["check", "operation", "array-clear"]
        );
    }

    #[test]
    fn android_secret_store_debug_is_redacted() {
        let bridge = Arc::new(RecordingSecretBridge::default());
        let store = AndroidSecretStore::from_bridge(bridge);
        let debug = format!("{store:?}");

        assert_eq!(debug, "AndroidSecretStore { bridge: \"[REDACTED]\" }");
    }

    #[test]
    fn android_secret_store_errors_do_not_expose_secret_text() {
        let bridge = Arc::new(RecordingSecretBridge::default());
        let store = AndroidSecretStore::from_bridge(bridge);
        let fixture = "task-five-secret-fixture";

        let error = store
            .set("provider/unsupported", SecretValue::new(fixture))
            .expect_err("unsupported key");
        assert!(!format!("{error:?}").contains(fixture));
        assert!(!error.to_string().contains(fixture));
    }
}
