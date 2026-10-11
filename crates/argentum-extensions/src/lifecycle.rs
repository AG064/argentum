use crate::{
    configuration::AdmittedDefinition,
    connection::{ConnectionError, McpSession},
};
use argentum_lifecycle::{
    EffectDisposer, EffectFailure, LifecycleKernel, ModuleSpec, MountedModule,
};
use async_trait::async_trait;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct OwnedConnection {
    pub record_revision: Uuid,
    pub session: Arc<Mutex<Option<McpSession>>>,
    pub cancellation: CancellationToken,
    pub admission: bool,
    mounted: MountedModule,
}

struct ConnectionDisposer {
    session: Arc<Mutex<Option<McpSession>>>,
    _leases: AdmittedDefinition,
}
#[async_trait]
impl EffectDisposer for ConnectionDisposer {
    async fn dispose(&mut self, _cancellation: &CancellationToken) -> Result<(), EffectFailure> {
        let mut session = self.session.lock().await;
        if let Some(value) = session.as_mut() {
            value
                .retire()
                .await
                .map_err(|_| EffectFailure::new("extension teardown remains pending"))?;
        }
        session.take();
        Ok(())
    }
}

/// One lifecycle owner. A caller removes admission and cancels the published
/// token before acquiring this mutex for teardown. Active calls then settle.
#[derive(Default)]
pub struct ExtensionLifecycle {
    kernel: LifecycleKernel,
    connections: BTreeMap<String, OwnedConnection>,
}
impl ExtensionLifecycle {
    pub fn connection(
        &self,
        namespace: &str,
        record_revision: Uuid,
    ) -> Option<(Arc<Mutex<Option<McpSession>>>, CancellationToken)> {
        let connection = self.connections.get(namespace)?;
        (connection.admission
            && connection.record_revision == record_revision
            && !connection.cancellation.is_cancelled())
        .then(|| {
            (
                Arc::clone(&connection.session),
                connection.cancellation.clone(),
            )
        })
    }
    pub fn cancel_admission(&mut self, namespace: &str) {
        if let Some(connection) = self.connections.get_mut(namespace) {
            connection.admission = false;
            connection.cancellation.cancel();
        }
    }
    pub async fn mount(
        &mut self,
        namespace: String,
        revision: Uuid,
        session: McpSession,
        admitted: AdmittedDefinition,
    ) -> Result<(), ConnectionError> {
        if self.connections.len() >= 32 || self.connections.contains_key(&namespace) {
            return Err(ConnectionError::Rejected(
                "connection_already_mounted_or_capacity_exceeded",
            ));
        }
        let cancellation = session.cancellation();
        let owned = Arc::new(Mutex::new(Some(session)));
        let session = Arc::clone(&owned);
        let binding = self.kernel.binding();
        use sha2::Digest;
        let module_id = format!("extension.{:x}", sha2::Sha256::digest(namespace.as_bytes()));
        let module_id = module_id[..42].to_owned();
        let mounted = self
            .kernel
            .mount(
                binding,
                ModuleSpec::new(module_id),
                &CancellationToken::new(),
                |context| {
                    Box::pin(async move {
                        context
                            .apply_effect("owned MCP session", || async move {
                                Ok(ConnectionDisposer {
                                    session,
                                    _leases: admitted,
                                })
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .map_err(|_| ConnectionError::Rejected("extension_lifecycle_mount_failed"))?;
        self.connections.insert(
            namespace,
            OwnedConnection {
                record_revision: revision,
                session: owned,
                cancellation,
                admission: true,
                mounted,
            },
        );
        Ok(())
    }
    pub async fn retire(&mut self, namespace: &str) -> Result<(), ConnectionError> {
        self.cancel_admission(namespace);
        let Some(connection) = self.connections.get(namespace) else {
            return Ok(());
        };
        let binding = self.kernel.binding();
        self.kernel
            .unmount(
                binding,
                connection.mounted.module(),
                connection.mounted.disposer(),
                &CancellationToken::new(),
            )
            .await
            .map_err(|_| ConnectionError::Rejected("extension_lifecycle_recovery_pending"))?;
        self.connections.remove(namespace);
        Ok(())
    }
    pub async fn shutdown(&mut self) -> Result<(), ConnectionError> {
        let names: Vec<_> = self.connections.keys().cloned().collect();
        for namespace in &names {
            self.cancel_admission(namespace);
        }
        for namespace in names {
            self.retire(&namespace).await?;
        }
        Ok(())
    }
}
