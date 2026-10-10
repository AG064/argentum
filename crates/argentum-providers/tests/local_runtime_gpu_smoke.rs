#![cfg(windows)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use argentum_domain::{ProviderKind, ProviderProfile};
use argentum_platform::PlatformAuthorityBroker;
use argentum_providers::{
    download_local_model, install_local_runtime, local_model_offers, local_runtime_offers,
    LocalLlamaCppProvider, LocalRuntimeConfig, LocalRuntimeLaunchApproval, LocalRuntimeModule,
    ModelProvider, ModelRequest, ProviderEvent, ProviderRegistry,
};
use argentum_security::{host_authority_channel, LocalAuthorityAction};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
#[ignore = "downloads pinned artifacts and requires a Windows Vulkan-capable GPU"]
async fn pinned_qwen_model_runs_through_the_contained_llama_cpp_sidecar(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("ARGENTUM_LOCAL_RUNTIME_SMOKE_ROOT")?);
    if !root.is_absolute() {
        return Err("ARGENTUM_LOCAL_RUNTIME_SMOKE_ROOT must be absolute".into());
    }
    let runtime_root = root.join("runtimes");
    let models_dir = root.join("models");
    let runtime_offer = local_runtime_offers()[0];
    let model_offer = local_model_offers()[0];
    let bundle = install_local_runtime(runtime_offer, &runtime_root).await?;
    let model = download_local_model(model_offer, &models_dir).await?;
    eprintln!("artifact integrity checks passed");
    assert_eq!(
        model.file_name().and_then(|name| name.to_str()),
        Some(model_offer.filename)
    );

    let (issuer, verifier, generation) = host_authority_channel()?;
    let platform_broker = Arc::new(PlatformAuthorityBroker::bootstrap(generation).await?);
    let module = Arc::new(LocalRuntimeModule::new(
        LocalRuntimeConfig::new(bundle, models_dir)?,
        Arc::clone(&platform_broker),
    ));
    let profile = ProviderProfile {
        id: "llama-cpp".into(),
        label: "llama.cpp".into(),
        kind: ProviderKind::LocalLlamaCpp,
        endpoint: "local://llama-cpp/".into(),
        model: model_offer.filename.into(),
        selected: true,
    };
    let prepared = module.prepare(&profile).await?;
    let sidecar_port = prepared.plan().port;
    let api_prefix = prepared.plan().api_prefix.clone();
    platform_broker
        .mount_dependent(
            "provider.local-runtime",
            ProviderRegistry::default().with_local_runtime(Arc::clone(&module)),
            &CancellationToken::new(),
        )
        .await?;
    let action = LocalAuthorityAction::ActivateLocalRuntime {
        plan: prepared.plan().digest(),
    };
    let generation = platform_broker
        .current_generation()
        .ok_or("platform authority generation is unavailable")?;
    let ticket = issuer.issue(action.clone(), generation.clone())?;
    let proof = verifier.consume(ticket, &action, generation.clone())?;
    let approval = LocalRuntimeLaunchApproval::bind(prepared.plan(), proof)?;
    let client = module
        .activate(prepared, Some(approval), CancellationToken::new())
        .await?;
    assert_eq!(client.model(), model_offer.filename);
    eprintln!("contained runtime activation passed");

    let provider = LocalLlamaCppProvider::with_runtime(model_offer.filename, Arc::clone(&module))?;
    let run_result = async {
        verify_reduced_unauthenticated_routes(sidecar_port, &api_prefix).await?;
        eprintln!("unauthenticated route reduction passed");
        let status = provider.probe().await?;
        eprintln!("authenticated provider probe passed");
        if !status.connected {
            return Err("local runtime probe did not report a connected provider".into());
        }
        let models = provider.list_models().await?;
        eprintln!("authenticated model catalog passed");
        if models.is_empty() {
            return Err("local runtime returned an empty model catalog".into());
        }

        let request = ModelRequest {
            require_tool_call: false,
            continuation_scope: None,
            output_limit: 4096,
            model: model_offer.filename.into(),
            system: Some("Answer briefly and do not call tools.".into()),
            history: Vec::new(),
            prompt: "Reply with the words Argentum local runtime ready.".into(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        };
        let (sender, mut receiver) = mpsc::channel(32);
        let stream = provider.stream_with_cancellation(request, sender, CancellationToken::new());
        tokio::pin!(stream);
        let mut output = String::new();
        let mut completed = false;
        tokio::time::timeout(Duration::from_secs(120), async {
            let mut stream_done = false;
            loop {
                tokio::select! {
                    result = &mut stream, if !stream_done => {
                        result?;
                        stream_done = true;
                    }
                    event = receiver.recv() => {
                        match event {
                            Some(ProviderEvent::Delta(delta)) => output.push_str(&delta),
                            Some(ProviderEvent::Completed) => completed = true,
                            Some(_) => {}
                            None if stream_done => {
                                break Ok::<(), argentum_providers::ProviderError>(());
                            }
                            None => {}
                        }
                    }
                }
            }
        })
        .await??;
        eprintln!("authenticated streamed completion returned");
        if !completed || output.trim().is_empty() {
            return Err("local runtime produced no completed text response".into());
        }
        verify_real_stream_cancellation(provider.clone(), model_offer.filename).await?;
        eprintln!("real local stream cancellation passed");
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    let shutdown_result = platform_broker.retire(&CancellationToken::new()).await;
    eprintln!("contained runtime shutdown returned");
    run_result?;
    shutdown_result?;
    Ok(())
}

async fn verify_reduced_unauthenticated_routes(
    port: u16,
    api_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(3))
        .build()?;
    let base = format!("http://127.0.0.1:{port}");
    let protected = client
        .get(format!("{base}{api_prefix}/v1/models"))
        .send()
        .await?;
    if protected.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Err(format!(
            "unauthenticated models route returned {} instead of 401",
            protected.status()
        )
        .into());
    }
    for route in ["/", "/slots", "/tools", "/mcp", "/props", "/metrics"] {
        let response = client
            .get(format!("{base}{api_prefix}{route}"))
            .send()
            .await?;
        if response.status().is_success() {
            return Err(format!("disabled route returned success: {route}").into());
        }
    }
    let root_health = client.get(format!("{base}/health")).send().await?;
    if root_health.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "unprefixed health route returned {} instead of 404",
            root_health.status()
        )
        .into());
    }

    let prefixed_health = client
        .get(format!("{base}{api_prefix}/health"))
        .send()
        .await?;
    if prefixed_health.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Err(format!(
            "generation-prefixed health route returned {} instead of 401",
            prefixed_health.status()
        )
        .into());
    }

    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0))?;
    socket.connect((std::net::Ipv4Addr::new(8, 8, 8, 8), 53))?;
    if let std::net::IpAddr::V4(address) = socket.local_addr()?.ip() {
        if !address.is_loopback() && !address.is_unspecified() {
            let result = client
                .get(format!("http://{address}:{port}/health"))
                .send()
                .await;
            if result.is_ok() {
                return Err("sidecar accepted a non-loopback address".into());
            }
        }
    }
    Ok(())
}

async fn verify_real_stream_cancellation(
    provider: LocalLlamaCppProvider,
    model: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = ModelRequest {
        require_tool_call: false,
        continuation_scope: None,
        output_limit: 4096,
        model: model.into(),
        system: Some("Continue until the request is cancelled. Do not call tools.".into()),
        history: Vec::new(),
        prompt: "Write a long numbered list with detailed explanations.".into(),
        tools: Vec::new(),
        tool_exchanges: Vec::new(),
    };
    let cancellation = CancellationToken::new();
    let task_cancellation = cancellation.clone();
    let (sender, mut receiver) = mpsc::channel(32);
    let task = tokio::spawn(async move {
        provider
            .stream_with_cancellation(request, sender, task_cancellation)
            .await
    });
    let first = tokio::time::timeout(Duration::from_secs(60), receiver.recv())
        .await?
        .ok_or("cancel stream closed before its first event")?;
    if matches!(first, ProviderEvent::Completed) {
        return Err("cancel stream completed before cancellation".into());
    }
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), task).await??;
    if !matches!(
        result,
        Err(argentum_providers::ProviderError::RequestCancelled)
    ) {
        return Err("local stream did not terminate as cancelled".into());
    }
    while receiver.try_recv().is_ok() {}
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    if receiver.try_recv().is_ok() {
        return Err("local stream emitted an event after cancellation".into());
    }
    Ok(())
}
