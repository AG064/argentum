use std::path::Path;
use tokio_util::sync::CancellationToken;

pub async fn run(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.is_empty() || arguments == ["help"] || arguments == ["--help"] {
        println!("Optional local document artifacts:\nartifacts tools\nartifacts inspect --config ABSOLUTE_FILE\nartifacts serve --config ABSOLUTE_FILE\nartifacts verify --config ABSOLUTE_FILE\nServe is the exact owner-activated MCP adapter, not a model command or authority source. Verification becomes native evidence only through an approved runner. No Google account or provider request is started by these commands.");
        return Ok(());
    }
    if arguments == ["tools"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&argentum_artifacts::server::tools())?
        );
        return Ok(());
    }
    if arguments.len() != 3 || arguments[1] != "--config" || !Path::new(&arguments[2]).is_absolute()
    {
        return Err("expected artifacts inspect|serve|verify --config ABSOLUTE_FILE".into());
    }
    let configuration = Path::new(&arguments[2]);
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let listener = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    let result: Result<(), Box<dyn std::error::Error>> = async {
        match arguments[0].as_str() {
            "inspect" => {
                let artifacts = argentum_artifacts::artifact::Artifacts::load(configuration)?;
                println!("{}", serde_json::to_string_pretty(&artifacts.open()?)?);
            }
            "serve" => argentum_artifacts::server::serve(configuration, cancel).await?,
            "verify" => println!(
                "{}",
                serde_json::to_string_pretty(
                    &argentum_artifacts::verify::verify(configuration, &cancel).await?
                )?
            ),
            _ => return Err("unknown artifact command".into()),
        }
        Ok(())
    }
    .await;
    listener.abort();
    let _ = listener.await;
    result
}
