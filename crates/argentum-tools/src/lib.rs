use std::collections::BTreeMap;
use std::sync::Arc;

use argentum_domain::{Capability, ToolDescriptor};
pub use argentum_domain::{ToolInput, ToolRequest};
use argentum_security::{ApprovalGrant, SecurityError};
use argentum_workspaces::{WorkspaceError, WorkspaceManager};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool is not registered: {0}")]
    NotRegistered(String),
    #[error("tool input is invalid: {0}")]
    InvalidInput(String),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("tool execution failed: {0}")]
    Execution(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub summary: String,
    pub output: String,
}

#[derive(Clone)]
pub struct ToolContext {
    pub workspace: WorkspaceManager,
    pub approval: ApprovalGrant,
}

#[async_trait]
pub trait AgentTool: Send + Sync {
    fn descriptor(&self) -> ToolDescriptor;

    async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError>;
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: Arc<BTreeMap<String, Arc<dyn AgentTool>>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistry {
    pub fn with_builtins(workspace: WorkspaceManager) -> Self {
        let mut registry = Self::default();
        registry.register(ReadTextTool);
        registry.register(WriteTextTool);
        let _ = workspace;
        registry
    }

    pub fn register<T>(&mut self, tool: T)
    where
        T: AgentTool + 'static,
    {
        let descriptor = tool.descriptor();
        let mut tools = (*self.tools).clone();
        tools.insert(descriptor.id, Arc::new(tool));
        self.tools = Arc::new(tools);
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools.values().map(|tool| tool.descriptor()).collect()
    }

    pub async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let tool_id = match &request.input {
            ToolInput::ReadText { .. } => "read_text",
            ToolInput::WriteText { .. } => "write_text",
        };
        let tool = self
            .tools
            .get(tool_id)
            .ok_or_else(|| ToolError::NotRegistered(tool_id.to_owned()))?;
        tool.execute(request, context).await
    }
}

pub struct ReadTextTool;

#[async_trait]
impl AgentTool for ReadTextTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "read_text".into(),
            title: "Read file".into(),
            summary: "Read a bounded text file inside the workspace".into(),
            capabilities: vec![Capability::ReadFiles],
            requires_approval: false,
        }
    }

    async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let ToolInput::ReadText { path } = request.input else {
            return Err(ToolError::InvalidInput("read_text requires a path".into()));
        };
        let output = context.workspace.read_text(path)?;
        let output = output.chars().take(32_000).collect::<String>();
        Ok(ToolResult {
            summary: "Read file".into(),
            output,
        })
    }
}

pub struct WriteTextTool;

#[async_trait]
impl AgentTool for WriteTextTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: "write_text".into(),
            title: "Write file".into(),
            summary: "Write text to a file inside the workspace".into(),
            capabilities: vec![Capability::WriteFiles],
            requires_approval: true,
        }
    }

    async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let ToolInput::WriteText { path, content } = request.input else {
            return Err(ToolError::InvalidInput(
                "write_text requires a path and content".into(),
            ));
        };
        context
            .workspace
            .write_text_with_grant(path, &content, &context.approval)?;
        Ok(ToolResult {
            summary: "Wrote file".into(),
            output: format!("{} bytes written", content.len()),
        })
    }
}
