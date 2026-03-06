# Tool Integration: acton-ai Built-in Tools

> Rust Implementation Plan - February 5, 2026
> Status: READY FOR IMPLEMENTATION

## Overview

Integrate acton-ai's built-in tools into the IPC handler's conversation flow, giving the LLM access to tools like `read_file`, `write_file`, `edit_file`, `list_directory`, `glob`, `grep`, `bash`, `calculate`, and `web_fetch` when processing user messages.

## Goals

1. **Update LLM Client**: Support OpenAI tool definitions in requests and handle tool_calls in responses
2. **Tool Registry**: Register acton-ai's BuiltinTools and make them available during conversation
3. **Tool Execution Loop**: Implement LLM response -> check tool calls -> execute tools -> send results back to LLM -> repeat
4. **Handler Integration**: Update DefaultIpcHandler to use tool-aware conversation flow

## Architecture

```
+-------------------------------------------------------------------+
|                     DefaultIpcHandler                              |
|  +-------------+  +-------------+  +-------------+                 |
|  |   LlmClient |  |   Tool      |  |   Tool      |                 |
|  |  (Ollama)   |  |   Registry  |  |   Executor  |                 |
|  +-------------+  +-------------+  +-------------+                 |
|        |               |                 |                         |
|        v               v                 v                         |
|  chat_with_tools() -> tool_calls -> execute() -> tool_results     |
|        ^                                          |                |
|        +------------------------------------------+                |
|               (loop until no more tool calls)                      |
+-------------------------------------------------------------------+
```

## Current State Analysis

### Existing Code

1. **LlmClient** (`crates/talon-core/src/llm/client.rs`):
   - Supports OpenAI-compatible API via Ollama
   - Has `ChatMessage` with role/content
   - Missing: tool_calls field, tool role support, tools parameter in requests

2. **DefaultIpcHandler** (`crates/talon-core/src/ipc/handlers.rs`):
   - Processes UserMessage via LlmClient
   - Uses simple streaming without tool support
   - Missing: tool execution loop, tool registry

3. **acton-ai BuiltinTools** (from `~/.cargo/registry/src/*/acton-ai-0.24.3/src/tools/builtins/`):
   - `BuiltinTools::all()` returns registry with all 9 tools
   - Each tool has `ToolConfig` with `ToolDefinition` (name, description, input_schema)
   - Tools implement `ToolExecutorTrait` with async `execute(args: Value) -> ToolExecutionFuture`
   - Tools are ready to use without actor spawning (direct executor pattern)

4. **acton-ai Message Types** (from `acton-ai/src/messages/types.rs`):
   - `ToolDefinition { name, description, input_schema }` - matches OpenAI format
   - `ToolCall { id, name, arguments }` - from LLM response
   - `Message` with `tool_calls` and `tool_call_id` fields

### Gaps to Fill

1. **ChatMessage Enhancement**: Add tool_calls/tool_call_id fields for conversation history
2. **Request Enhancement**: Add tools array to chat completion requests
3. **Response Parsing**: Parse tool_calls from streaming/non-streaming responses
4. **Tool Executor**: Create wrapper to execute BuiltinTools with error handling
5. **Conversation Loop**: Implement multi-turn tool execution until completion

## Implementation Tasks

### Task 1: Update ChatMessage for Tool Support

**File**: `crates/talon-core/src/llm/client.rs`

Update `ChatMessage` to support tool calls and tool responses.

```rust
use serde::{Deserialize, Serialize};

/// A tool call from the LLM
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Type of the call (always "function" for now)
    #[serde(rename = "type")]
    pub call_type: String,
    /// The function details
    pub function: FunctionCall,
}

/// Function call details
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Name of the function to call
    pub name: String,
    /// JSON-encoded arguments
    pub arguments: String,
}

/// A single message in a conversation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message sender ("system", "user", "assistant", or "tool")
    pub role: String,
    /// Content of the message (may be empty for tool call messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls made by the assistant (only for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// ID of the tool call this message responds to (only for tool messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// Create a system message
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a user message
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message with tool calls
    #[must_use]
    pub fn assistant_with_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    /// Create a tool response message
    #[must_use]
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}
```

### Task 2: Create Tool Definition Type

**File**: `crates/talon-core/src/llm/client.rs`

Add OpenAI-compatible tool definition for requests.

```rust
/// Tool definition for the LLM
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Type of the tool (always "function")
    #[serde(rename = "type")]
    pub tool_type: String,
    /// The function details
    pub function: FunctionDefinition,
}

/// Function definition for tool
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Name of the function
    pub name: String,
    /// Description of what the function does
    pub description: String,
    /// JSON Schema for function parameters
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    /// Create a new tool definition
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }

    /// Create from acton-ai ToolDefinition
    #[must_use]
    pub fn from_acton(def: &acton_ai::messages::ToolDefinition) -> Self {
        Self::new(&def.name, &def.description, def.input_schema.clone())
    }
}
```

### Task 3: Update Chat Completion Request

**File**: `crates/talon-core/src/llm/client.rs`

Update request structure to include tools.

```rust
/// Request body for chat completions
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}
```

### Task 4: Update Response Parsing for Tool Calls

**File**: `crates/talon-core/src/llm/client.rs`

Update response structures to handle tool calls.

```rust
/// Delta content in streaming response
#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<StreamToolCall>>,
}

/// Tool call in streaming response (partial)
#[derive(Debug, Deserialize)]
struct StreamToolCall {
    index: usize,
    id: Option<String>,
    #[serde(rename = "type")]
    call_type: Option<String>,
    function: Option<StreamFunctionCall>,
}

/// Function call in streaming response (partial)
#[derive(Debug, Deserialize)]
struct StreamFunctionCall {
    name: Option<String>,
    arguments: Option<String>,
}

/// Choice in streaming response
#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

/// Streaming response chunk
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

/// Events from the streaming response
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A token was received
    Token(String),
    /// A tool call was received
    ToolCall(ToolCall),
    /// The stream completed normally
    Done,
    /// The stream completed requesting tool use
    ToolUse(Vec<ToolCall>),
    /// An error occurred
    Error(String),
}
```

### Task 5: Add Tool-Aware Chat Method

**File**: `crates/talon-core/src/llm/client.rs`

Add new method for chat with tools.

```rust
impl LlmClient {
    /// Send a chat completion request with tools
    ///
    /// # Arguments
    ///
    /// * `messages` - The conversation history
    /// * `tools` - Available tool definitions
    ///
    /// # Returns
    ///
    /// A stream of `StreamEvent` items.
    pub fn chat_stream_with_tools(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<ToolDefinition>>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send>> {
        let url = format!("{}/v1/chat/completions", self.config.base_url);
        let client = self.client.clone();
        let model = self.config.model.clone();
        let system_prompt = self.config.system_prompt.clone();

        Box::pin(async_stream::stream! {
            // Prepend system message if configured and not already present
            let mut all_messages = Vec::new();
            let has_system = messages.iter().any(|m| m.role == "system");
            if !has_system {
                if let Some(prompt) = system_prompt {
                    all_messages.push(ChatMessage::system(prompt));
                }
            }
            all_messages.extend(messages);

            let request = ChatCompletionRequest {
                model,
                messages: all_messages,
                stream: true,
                tools,
                tool_choice: None, // Let the model decide
            };

            debug!(url = %url, "sending chat completion request with tools");

            let response = match client.post(&url).json(&request).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    error!(error = %e, "failed to send request");
                    yield StreamEvent::Error(format!("request failed: {e}"));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                error!(status = %status, body = %body, "request failed");
                yield StreamEvent::Error(format!("server error {status}: {body}"));
                return;
            }

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
            let mut tool_call_builders: std::collections::HashMap<usize, ToolCallBuilder> =
                std::collections::HashMap::new();

            while let Some(chunk_result) = stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        warn!(error = %e, "error reading stream");
                        yield StreamEvent::Error(format!("stream error: {e}"));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if json_str == "[DONE]" {
                            // Check if we have pending tool calls
                            if !tool_call_builders.is_empty() {
                                let mut tool_calls: Vec<ToolCall> = tool_call_builders
                                    .into_values()
                                    .filter_map(|b| b.build())
                                    .collect();
                                tool_calls.sort_by_key(|tc| tc.id.clone());
                                if !tool_calls.is_empty() {
                                    yield StreamEvent::ToolUse(tool_calls);
                                    return;
                                }
                            }
                            yield StreamEvent::Done;
                            return;
                        }

                        match serde_json::from_str::<StreamChunk>(json_str) {
                            Ok(chunk) => {
                                for choice in chunk.choices {
                                    // Handle content tokens
                                    if let Some(content) = choice.delta.content {
                                        if !content.is_empty() {
                                            yield StreamEvent::Token(content);
                                        }
                                    }

                                    // Handle tool calls (accumulated across chunks)
                                    if let Some(tool_calls) = choice.delta.tool_calls {
                                        for tc in tool_calls {
                                            let builder = tool_call_builders
                                                .entry(tc.index)
                                                .or_insert_with(ToolCallBuilder::new);
                                            builder.update(tc);
                                        }
                                    }

                                    // Check finish reason
                                    if let Some(reason) = choice.finish_reason {
                                        if reason == "tool_calls" {
                                            let mut tool_calls: Vec<ToolCall> = tool_call_builders
                                                .drain()
                                                .filter_map(|(_, b)| b.build())
                                                .collect();
                                            tool_calls.sort_by_key(|tc| tc.id.clone());
                                            if !tool_calls.is_empty() {
                                                yield StreamEvent::ToolUse(tool_calls);
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                debug!(error = %e, json = %json_str, "failed to parse chunk");
                            }
                        }
                    }
                }
            }

            // If we get here without explicit done, check for tool calls
            if !tool_call_builders.is_empty() {
                let mut tool_calls: Vec<ToolCall> = tool_call_builders
                    .into_values()
                    .filter_map(|b| b.build())
                    .collect();
                tool_calls.sort_by_key(|tc| tc.id.clone());
                if !tool_calls.is_empty() {
                    yield StreamEvent::ToolUse(tool_calls);
                    return;
                }
            }

            yield StreamEvent::Done;
        })
    }
}

/// Builder for accumulating tool call chunks
#[derive(Default)]
struct ToolCallBuilder {
    id: Option<String>,
    call_type: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl ToolCallBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn update(&mut self, tc: StreamToolCall) {
        if let Some(id) = tc.id {
            self.id = Some(id);
        }
        if let Some(ct) = tc.call_type {
            self.call_type = Some(ct);
        }
        if let Some(f) = tc.function {
            if let Some(name) = f.name {
                self.name = Some(name);
            }
            if let Some(args) = f.arguments {
                self.arguments.push_str(&args);
            }
        }
    }

    fn build(self) -> Option<ToolCall> {
        Some(ToolCall {
            id: self.id?,
            call_type: self.call_type.unwrap_or_else(|| "function".to_string()),
            function: FunctionCall {
                name: self.name?,
                arguments: self.arguments,
            },
        })
    }
}
```

### Task 6: Create Tool Executor Module

**File**: `crates/talon-core/src/llm/tools.rs` (NEW)

Create a module to execute acton-ai tools.

```rust
//! Tool execution for LLM conversations
//!
//! Provides integration with acton-ai's built-in tools for LLM tool calling.

use std::collections::HashMap;
use std::sync::Arc;

use acton_ai::tools::builtins::BuiltinTools;
use acton_ai::tools::{BoxedToolExecutor, ToolConfig};
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::error::{TalonError, TalonResult};
use crate::llm::{FunctionCall, ToolCall, ToolDefinition};

/// Default tools to enable
pub const DEFAULT_TOOLS: &[&str] = &[
    "read_file",
    "write_file",
    "edit_file",
    "list_directory",
    "glob",
    "grep",
    "bash",
    "calculate",
    "web_fetch",
];

/// Tool executor for LLM tool calling
pub struct ToolExecutor {
    /// Tool configurations by name
    configs: HashMap<String, ToolConfig>,
    /// Tool executors by name
    executors: HashMap<String, Arc<BoxedToolExecutor>>,
}

impl ToolExecutor {
    /// Create a new tool executor with all default tools
    #[must_use]
    pub fn new() -> Self {
        Self::with_tools(DEFAULT_TOOLS)
    }

    /// Create a tool executor with specific tools
    #[must_use]
    pub fn with_tools(tool_names: &[&str]) -> Self {
        let builtins = BuiltinTools::all();
        let mut configs = HashMap::new();
        let mut executors = HashMap::new();

        for name in tool_names {
            if let (Some(config), Some(executor)) = (
                builtins.get_config(name),
                builtins.get_executor(name),
            ) {
                configs.insert((*name).to_string(), config.clone());
                executors.insert((*name).to_string(), executor);
                debug!(tool = name, "registered tool");
            } else {
                warn!(tool = name, "tool not found in builtins");
            }
        }

        info!(count = configs.len(), "tool executor initialized");

        Self { configs, executors }
    }

    /// Get tool definitions for LLM request
    #[must_use]
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.configs
            .values()
            .map(|config| {
                ToolDefinition::new(
                    &config.definition.name,
                    &config.definition.description,
                    config.definition.input_schema.clone(),
                )
            })
            .collect()
    }

    /// Execute a single tool call
    ///
    /// # Arguments
    ///
    /// * `tool_call` - The tool call from the LLM
    ///
    /// # Returns
    ///
    /// The result as a JSON string, or an error message.
    pub async fn execute(&self, tool_call: &ToolCall) -> String {
        let tool_name = &tool_call.function.name;

        debug!(
            tool = tool_name,
            id = tool_call.id,
            "executing tool call"
        );

        let Some(executor) = self.executors.get(tool_name) else {
            let error_msg = format!("Unknown tool: {tool_name}");
            error!(tool = tool_name, "tool not found");
            return serde_json::json!({ "error": error_msg }).to_string();
        };

        // Parse arguments
        let args: Value = match serde_json::from_str(&tool_call.function.arguments) {
            Ok(v) => v,
            Err(e) => {
                let error_msg = format!("Invalid arguments: {e}");
                error!(tool = tool_name, error = %e, "failed to parse arguments");
                return serde_json::json!({ "error": error_msg }).to_string();
            }
        };

        // Execute the tool
        match executor.execute(args).await {
            Ok(result) => {
                debug!(tool = tool_name, "tool execution successful");
                serde_json::to_string(&result).unwrap_or_else(|e| {
                    serde_json::json!({ "error": format!("Failed to serialize result: {e}") })
                        .to_string()
                })
            }
            Err(e) => {
                error!(tool = tool_name, error = %e, "tool execution failed");
                serde_json::json!({ "error": e.to_string() }).to_string()
            }
        }
    }

    /// Execute multiple tool calls in parallel
    ///
    /// # Arguments
    ///
    /// * `tool_calls` - The tool calls from the LLM
    ///
    /// # Returns
    ///
    /// Results for each tool call, in order.
    pub async fn execute_all(&self, tool_calls: &[ToolCall]) -> Vec<(String, String)> {
        let futures: Vec<_> = tool_calls
            .iter()
            .map(|tc| {
                let id = tc.id.clone();
                let executor = self.clone();
                let tc = tc.clone();
                async move {
                    let result = executor.execute(&tc).await;
                    (id, result)
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    /// Check if a tool is available
    #[must_use]
    pub fn has_tool(&self, name: &str) -> bool {
        self.executors.contains_key(name)
    }

    /// Get list of available tool names
    #[must_use]
    pub fn tool_names(&self) -> Vec<&str> {
        self.configs.keys().map(String::as_str).collect()
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ToolExecutor {
    fn clone(&self) -> Self {
        Self {
            configs: self.configs.clone(),
            executors: self.executors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_executor_creates_with_defaults() {
        let executor = ToolExecutor::new();
        assert!(executor.has_tool("read_file"));
        assert!(executor.has_tool("glob"));
        assert!(executor.has_tool("calculate"));
    }

    #[test]
    fn tool_executor_creates_with_specific_tools() {
        let executor = ToolExecutor::with_tools(&["read_file", "glob"]);
        assert!(executor.has_tool("read_file"));
        assert!(executor.has_tool("glob"));
        assert!(!executor.has_tool("bash"));
    }

    #[test]
    fn tool_definitions_match_tools() {
        let executor = ToolExecutor::with_tools(&["calculate"]);
        let definitions = executor.tool_definitions();

        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].function.name, "calculate");
    }

    #[tokio::test]
    async fn execute_unknown_tool_returns_error() {
        let executor = ToolExecutor::with_tools(&["calculate"]);
        let tool_call = ToolCall {
            id: "tc_1".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "unknown_tool".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let result = executor.execute(&tool_call).await;
        assert!(result.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn execute_calculate_tool() {
        let executor = ToolExecutor::with_tools(&["calculate"]);
        let tool_call = ToolCall {
            id: "tc_1".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "calculate".to_string(),
                arguments: r#"{"expression": "2 + 2"}"#.to_string(),
            },
        };

        let result = executor.execute(&tool_call).await;
        // The calculate tool should return the result
        assert!(!result.contains("error") || result.contains("4"));
    }
}
```

### Task 7: Create Tool-Aware Conversation Handler

**File**: `crates/talon-core/src/llm/conversation.rs` (NEW)

Create a module for tool-aware conversation management.

```rust
//! Tool-aware conversation handling
//!
//! Implements the LLM -> tool call -> result -> LLM loop for multi-turn
//! tool-using conversations.

use futures::StreamExt;
use tracing::{debug, info, warn};

use crate::error::{TalonError, TalonResult};
use crate::llm::{ChatMessage, LlmClient, StreamEvent, ToolCall, ToolDefinition};
use crate::llm::tools::ToolExecutor;

/// Maximum number of tool execution rounds to prevent infinite loops
const MAX_TOOL_ROUNDS: usize = 10;

/// Result of a conversation turn
#[derive(Debug)]
pub struct ConversationResult {
    /// The final assistant response content
    pub content: String,
    /// Tool calls made during the conversation (for logging/debugging)
    pub tool_calls_made: Vec<ToolCallRecord>,
}

/// Record of a tool call made during conversation
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    /// Tool call ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Tool arguments (JSON)
    pub arguments: String,
    /// Tool result (JSON)
    pub result: String,
}

/// Conversation handler with tool support
pub struct ConversationHandler {
    /// The LLM client
    llm_client: LlmClient,
    /// The tool executor
    tool_executor: ToolExecutor,
}

impl ConversationHandler {
    /// Create a new conversation handler
    ///
    /// # Arguments
    ///
    /// * `llm_client` - The LLM client
    /// * `tool_executor` - The tool executor
    #[must_use]
    pub fn new(llm_client: LlmClient, tool_executor: ToolExecutor) -> Self {
        Self {
            llm_client,
            tool_executor,
        }
    }

    /// Process a user message with tool support
    ///
    /// This implements the tool execution loop:
    /// 1. Send message to LLM with tool definitions
    /// 2. If LLM returns tool calls, execute them
    /// 3. Send tool results back to LLM
    /// 4. Repeat until LLM returns final response or max rounds reached
    ///
    /// # Arguments
    ///
    /// * `user_content` - The user's message
    /// * `conversation_history` - Optional previous messages for context
    ///
    /// # Returns
    ///
    /// The conversation result including the final response and tool call records.
    ///
    /// # Errors
    ///
    /// Returns error if LLM communication fails or max rounds exceeded.
    pub async fn process_message(
        &self,
        user_content: &str,
        conversation_history: Option<Vec<ChatMessage>>,
    ) -> TalonResult<ConversationResult> {
        let mut messages = conversation_history.unwrap_or_default();
        messages.push(ChatMessage::user(user_content));

        let tool_definitions = Some(self.tool_executor.tool_definitions());
        let mut tool_calls_made = Vec::new();
        let mut round = 0;

        loop {
            round += 1;
            if round > MAX_TOOL_ROUNDS {
                warn!("max tool rounds exceeded");
                return Err(TalonError::ActonAI {
                    message: format!(
                        "exceeded maximum tool execution rounds ({MAX_TOOL_ROUNDS})"
                    ),
                });
            }

            debug!(round, "starting conversation round");

            // Get LLM response
            let mut stream = self
                .llm_client
                .chat_stream_with_tools(messages.clone(), tool_definitions.clone());

            let mut response_content = String::new();
            let mut pending_tool_calls: Option<Vec<ToolCall>> = None;

            while let Some(event) = stream.next().await {
                match event {
                    StreamEvent::Token(token) => {
                        response_content.push_str(&token);
                    }
                    StreamEvent::Done => {
                        break;
                    }
                    StreamEvent::ToolUse(tool_calls) => {
                        pending_tool_calls = Some(tool_calls);
                        break;
                    }
                    StreamEvent::Error(e) => {
                        return Err(TalonError::ActonAI {
                            message: format!("LLM error: {e}"),
                        });
                    }
                    StreamEvent::ToolCall(_) => {
                        // Individual tool calls are accumulated internally
                    }
                }
            }

            // Check if we have tool calls to execute
            if let Some(tool_calls) = pending_tool_calls {
                info!(
                    count = tool_calls.len(),
                    round,
                    "executing tool calls"
                );

                // Add assistant message with tool calls to history
                messages.push(ChatMessage::assistant_with_tool_calls(
                    if response_content.is_empty() {
                        None
                    } else {
                        Some(response_content.clone())
                    },
                    tool_calls.clone(),
                ));

                // Execute all tool calls
                let results = self.tool_executor.execute_all(&tool_calls).await;

                // Add tool results to history and records
                for (id, result) in results {
                    // Find the corresponding tool call for logging
                    if let Some(tc) = tool_calls.iter().find(|tc| tc.id == id) {
                        tool_calls_made.push(ToolCallRecord {
                            id: id.clone(),
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                            result: result.clone(),
                        });

                        debug!(
                            tool = tc.function.name,
                            id,
                            "tool execution complete"
                        );
                    }

                    // Add tool response to messages
                    messages.push(ChatMessage::tool(id, result));
                }

                // Continue the loop to get LLM's next response
                continue;
            }

            // No tool calls - we have our final response
            info!(
                round,
                tool_calls = tool_calls_made.len(),
                "conversation complete"
            );

            return Ok(ConversationResult {
                content: response_content,
                tool_calls_made,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmClientConfig;

    // Note: These tests require a running Ollama instance
    // They are marked as ignored by default

    #[tokio::test]
    #[ignore = "requires running Ollama"]
    async fn test_simple_message_without_tools() {
        let client = LlmClient::new(LlmClientConfig::default()).expect("client");
        let executor = ToolExecutor::with_tools(&[]); // No tools
        let handler = ConversationHandler::new(client, executor);

        let result = handler
            .process_message("What is 2 + 2?", None)
            .await
            .expect("result");

        assert!(!result.content.is_empty());
        assert!(result.tool_calls_made.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires running Ollama"]
    async fn test_message_with_tool_use() {
        let client = LlmClient::new(LlmClientConfig::default()).expect("client");
        let executor = ToolExecutor::with_tools(&["calculate"]);
        let handler = ConversationHandler::new(client, executor);

        let result = handler
            .process_message("Use the calculate tool to compute 2 + 2", None)
            .await
            .expect("result");

        // The LLM should have used the calculate tool
        assert!(!result.content.is_empty());
        // Note: The LLM may or may not use the tool depending on the model
    }
}
```

### Task 8: Update LLM Module Exports

**File**: `crates/talon-core/src/llm/mod.rs`

Update module exports.

```rust
//! LLM client for Ollama integration
//!
//! Provides an OpenAI-compatible HTTP client for communicating with Ollama.
//! Supports both streaming and non-streaming completions, with tool calling support.

mod client;
mod conversation;
mod tools;

pub use client::{
    ChatMessage, FunctionCall, FunctionDefinition, LlmClient, LlmClientConfig,
    StreamEvent, ToolCall, ToolDefinition,
};
pub use conversation::{ConversationHandler, ConversationResult, ToolCallRecord};
pub use tools::{ToolExecutor, DEFAULT_TOOLS};
```

### Task 9: Update IPC Handler for Tool Support

**File**: `crates/talon-core/src/ipc/handlers.rs`

Update the handler to use tool-aware conversation.

```rust
// Add imports
use crate::llm::{ConversationHandler, ToolExecutor};

/// Default IPC message handler implementation
pub struct DefaultIpcHandler {
    /// Token authenticator
    authenticator: TokenAuthenticator,
    /// Map of authenticated channels to their validated tokens
    authenticated_channels: Arc<DashMap<String, ValidatedToken>>,
    /// LLM client for processing messages
    llm_client: Option<Arc<LlmClient>>,
    /// Conversation handler with tool support
    conversation_handler: Option<Arc<ConversationHandler>>,
}

impl DefaultIpcHandler {
    /// Create a new handler with a token authenticator
    #[must_use]
    pub fn new(authenticator: TokenAuthenticator) -> Self {
        Self {
            authenticator,
            authenticated_channels: Arc::new(DashMap::new()),
            llm_client: None,
            conversation_handler: None,
        }
    }

    /// Create a new handler with a token authenticator and LLM client
    #[must_use]
    pub fn with_llm(authenticator: TokenAuthenticator, llm_client: Arc<LlmClient>) -> Self {
        // Create tool executor with default tools
        let tool_executor = ToolExecutor::new();

        // Create conversation handler
        let conversation_handler = ConversationHandler::new(
            (*llm_client).clone(),
            tool_executor,
        );

        Self {
            authenticator,
            authenticated_channels: Arc::new(DashMap::new()),
            llm_client: Some(llm_client),
            conversation_handler: Some(Arc::new(conversation_handler)),
        }
    }

    /// Create handler with custom tool configuration
    #[must_use]
    pub fn with_llm_and_tools(
        authenticator: TokenAuthenticator,
        llm_client: Arc<LlmClient>,
        tool_names: &[&str],
    ) -> Self {
        let tool_executor = ToolExecutor::with_tools(tool_names);
        let conversation_handler = ConversationHandler::new(
            (*llm_client).clone(),
            tool_executor,
        );

        Self {
            authenticator,
            authenticated_channels: Arc::new(DashMap::new()),
            llm_client: Some(llm_client),
            conversation_handler: Some(Arc::new(conversation_handler)),
        }
    }

    // ... existing methods ...
}

#[async_trait]
impl IpcMessageHandler for DefaultIpcHandler {
    async fn handle(&self, message: ChannelToCore) -> TalonResult<CoreToChannel> {
        match message {
            // ... existing handlers ...

            ChannelToCore::UserMessage {
                correlation_id,
                conversation_id,
                sender,
                content,
            } => {
                // Extract channel ID from sender
                let channel_id = &sender.channel_id;

                // Require authentication
                self.require_auth(channel_id)?;

                debug!(
                    correlation_id = %correlation_id,
                    conversation_id = %conversation_id,
                    channel = %channel_id,
                    "processing user message with tool support"
                );

                // Process with conversation handler if available
                if let Some(handler) = &self.conversation_handler {
                    match handler.process_message(&content, None).await {
                        Ok(result) => {
                            if !result.tool_calls_made.is_empty() {
                                debug!(
                                    tool_calls = result.tool_calls_made.len(),
                                    "completed with tool calls"
                                );
                            }

                            Ok(CoreToChannel::Complete {
                                correlation_id,
                                conversation_id,
                                content: result.content,
                            })
                        }
                        Err(e) => {
                            error!(error = %e, "conversation handler error");
                            Ok(CoreToChannel::Error {
                                correlation_id,
                                message: format!("Processing error: {e}"),
                            })
                        }
                    }
                } else if let Some(llm) = &self.llm_client {
                    // Fallback to streaming without tools
                    let messages = vec![ChatMessage::user(&content)];
                    let mut stream = llm.chat_stream(messages);
                    let mut response_content = String::new();

                    while let Some(event) = stream.next().await {
                        match event {
                            StreamEvent::Token(token) => {
                                response_content.push_str(&token);
                            }
                            StreamEvent::Done => {
                                break;
                            }
                            StreamEvent::Error(e) => {
                                error!(error = %e, "LLM stream error");
                                return Ok(CoreToChannel::Error {
                                    correlation_id,
                                    message: format!("LLM error: {e}"),
                                });
                            }
                            _ => {}
                        }
                    }

                    Ok(CoreToChannel::Complete {
                        correlation_id,
                        conversation_id,
                        content: response_content,
                    })
                } else {
                    debug!("no LLM configured, echoing message");
                    Ok(CoreToChannel::Complete {
                        correlation_id,
                        conversation_id,
                        content: format!("Echo (no LLM): {content}"),
                    })
                }
            }
        }
    }

    // ... existing methods ...
}
```

### Task 10: Update lib.rs Exports

**File**: `crates/talon-core/src/lib.rs`

Update public exports.

```rust
//! Talon Core - Secure multi-channel AI assistant daemon
//!
//! This crate provides the core runtime including:
//! - Router actor for IPC communication
//! - Conversation management with tool support
//! - SecureSkillRegistry with attestation verification
//! - Trust tier enforcement
//! - Capability-verified tool execution
//! - Token-based channel authentication

pub mod config;
pub mod conversation;
pub mod error;
pub mod ipc;
pub mod llm;
pub mod router;
pub mod runtime;
pub mod skills;
pub mod trust;
pub mod types;

pub use config::TalonConfig;
pub use error::{TalonError, TalonResult};
pub use ipc::{ChannelToCore, CoreToChannel};
pub use llm::{
    ChatMessage, ConversationHandler, LlmClient, LlmClientConfig,
    StreamEvent, ToolCall, ToolDefinition, ToolExecutor,
};
pub use runtime::{RuntimeConfig, RuntimeConfigBuilder, TalonRuntime};
pub use types::*;
```

## Custom Error Types

No new error types needed. Existing `TalonError` variants handle all cases:
- `TalonError::ActonAI` - LLM and tool execution errors
- `TalonError::ToolExecution` - Individual tool failures

## Test Strategy

### Unit Tests

1. **ChatMessage Construction**
   - Test all message constructors
   - Test serialization/deserialization
   - Test tool_calls and tool_call_id fields

2. **ToolDefinition Conversion**
   - Test from_acton conversion
   - Test serialization matches OpenAI format

3. **ToolExecutor**
   - Test tool registration
   - Test tool_definitions generation
   - Test execute with unknown tool
   - Test execute with invalid arguments

4. **ToolCallBuilder**
   - Test chunk accumulation
   - Test build with complete data
   - Test build with incomplete data

### Integration Tests

1. **Conversation Handler**
   - Test simple message without tools (requires Ollama)
   - Test message that triggers tool use (requires Ollama)
   - Test max rounds protection

2. **IPC Handler Integration**
   - Test handler with tool support enabled
   - Test handler with custom tools

## Files to Create/Modify

| File | Action | Purpose |
|------|--------|---------|
| `crates/talon-core/src/llm/client.rs` | Modify | Add tool support to ChatMessage, requests, responses |
| `crates/talon-core/src/llm/tools.rs` | Create | Tool executor wrapping acton-ai BuiltinTools |
| `crates/talon-core/src/llm/conversation.rs` | Create | Tool-aware conversation handler |
| `crates/talon-core/src/llm/mod.rs` | Modify | Export new modules |
| `crates/talon-core/src/ipc/handlers.rs` | Modify | Use conversation handler with tools |
| `crates/talon-core/src/lib.rs` | Modify | Export new types |

## Dependencies

No new dependencies. Uses existing:
- `acton-ai` with `agent-skills` feature (already in workspace)
- `futures` for stream handling
- `serde` / `serde_json` for serialization
- `tracing` for logging

## Semver Recommendation

**Minor version bump (0.2.0 -> 0.3.0)**

Justification:
- New public APIs (ToolExecutor, ConversationHandler, new ChatMessage fields)
- New functionality (tool execution, conversation loop)
- Backward compatible - existing code continues to work
- DefaultIpcHandler.with_llm() behavior changes but maintains same interface
