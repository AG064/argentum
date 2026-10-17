//! Optional maintained MCP server over the existing bounded stdio transport.
//! Construction and discovery are local-only; no credential or provider path.
use crate::{artifact::Artifacts, ArtifactError as Error, Result};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig,
        Tool,
    },
    service::RequestContext,
    ErrorData, RoleServer, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const TOOL_NAMES: [&str; 3] = ["artifact_open", "artifact_render", "artifact_page"];

pub fn tools() -> Value {
    let digest = json!({"type":"string","pattern":"^[0-9a-fA-F]{64}$"});
    json!({"tools":[
        {"name":"artifact_open","description":"Observe the owner-selected local inputs, editable Markdown path, exact current SHA and artifact selection. Use ordinary read and guarded write/edit tools for authoring. This does not render, start a model or verify correctness.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"artifact_render","description":"Create and render a bounded PDF from the exact current Markdown SHA. Pass the artifact selection returned by artifact_open, or null if absent. Existing generations remain retained. Supports headings 1-3, prose, flat lists, rectangular tables and literal code; precomposed Latin/Greek/Cyrillic. Rejects raw HTML, links, images, inline emphasis and unsupported shaping. Inspect every returned page tile, then obtain current native verification. Success is not semantic or visual acceptance.",
            "inputSchema":{"type":"object","properties":{"source_sha256":digest,"expected_artifact":{"type":["string","null"],"format":"uuid"}},
                "required":["source_sha256","expected_artifact"],"additionalProperties":false}},
        {"name":"artifact_page","description":"Prepare one actual current PDF page tile for the host's existing model-image path. Requires the exact returned artifact ID and page/tile numbers. A changed source, PDF or image invalidates it. Preparation is not transport acceptance or proof of visual comprehension.",
            "inputSchema":{"type":"object","properties":{"artifact_id":{"type":"string","format":"uuid"},"page":{"type":"integer","minimum":1,"maximum":16},
                "tile":{"type":"integer","minimum":1,"maximum":5}},"required":["artifact_id","page","tile"],"additionalProperties":false}}
    ]})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Render {
    source_sha256: String,
    #[serde(deserialize_with = "required_optional_uuid")]
    expected_artifact: Option<Uuid>,
}
fn required_optional_uuid<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Uuid>, D::Error> {
    Option::deserialize(deserializer)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    artifact_id: Uuid,
    page: u32,
    tile: u32,
}

struct Handler {
    artifacts: Mutex<Artifacts>,
}
impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "argentum-local-artifacts",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        let catalog = tools();
        catalog["tools"]
            .as_array()?
            .iter()
            .find(|t| t["name"] == name)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        serde_json::from_value(tools())
            .map_err(|_| ErrorData::internal_error("artifact catalog unavailable", None))
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let invalid = || {
            Error::Contract(
                "arguments do not match the advertised artifact schema; no action was inferred",
            )
        };
        let result: Result<Value> = async {
            let artifacts = tokio::select! {biased;
                _=context.ct.cancelled()=>return Err(Error::Cancelled),
                value=self.artifacts.lock()=>value,
            };
            match request.name.as_ref() {
                "artifact_open" => {
                    let _: Empty = serde_json::from_value(arguments).map_err(|_| invalid())?;
                    artifacts.open()
                }
                "artifact_render" => {
                    let args: Render = serde_json::from_value(arguments).map_err(|_| invalid())?;
                    if !crate::artifact::digest_valid(&args.source_sha256) {
                        return Err(invalid());
                    }
                    artifacts
                        .render(&args.source_sha256, args.expected_artifact, &context.ct)
                        .await
                }
                "artifact_page" => {
                    let args: Page = serde_json::from_value(arguments).map_err(|_| invalid())?;
                    artifacts.page(args.artifact_id, args.page, args.tile)
                }
                _ => Err(Error::Contract(
                    "tool is not in the local artifact operation set",
                )),
            }
        }
        .await;
        let result = match result {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        };
        Ok(result.into())
    }
}

pub async fn serve(configuration: &Path, cancel: CancellationToken) -> Result<()> {
    let handler = Handler {
        artifacts: Mutex::new(Artifacts::load(configuration)?),
    };
    let wire = argentum_extensions::wire::BoundedStdio::new_server(
        tokio::io::stdin(),
        tokio::io::stdout(),
    );
    let service = handler
        .serve_with_ct(wire, cancel)
        .await
        .map_err(|_| Error::Contract("artifact MCP connection failed"))?;
    service
        .waiting()
        .await
        .map_err(|_| Error::Contract("artifact MCP connection did not settle"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schemas_and_validators_require_explicit_nullable_selection() {
        let sha = "a".repeat(64);
        assert!(serde_json::from_value::<Render>(json!({"source_sha256":sha})).is_err());
        assert!(serde_json::from_value::<Render>(
            json!({"source_sha256":sha,"expected_artifact":null})
        )
        .is_ok());
        assert!(serde_json::from_value::<Render>(
            json!({"source_sha256":sha,"expected_artifact":null,"file":"outside"})
        )
        .is_err());
        assert!(serde_json::from_value::<Empty>(json!({"authorize":true})).is_err());
        let handler = tools();
        let tools = handler["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(
            tools[1]["inputSchema"]["required"],
            json!(["source_sha256", "expected_artifact"])
        );
        for tool in tools {
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
    }
}
