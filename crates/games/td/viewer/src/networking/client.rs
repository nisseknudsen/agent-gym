//! MCP HTTP client using ehttp.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// JSON-RPC request structure.
#[derive(Serialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'static str,
    pub params: T,
}

/// JSON-RPC response structure.
#[derive(Deserialize, Debug)]
pub struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    pub jsonrpc: String,
    #[allow(dead_code)]
    pub id: u64,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

#[derive(Deserialize, Debug)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

/// MCP tool call parameters.
#[derive(Serialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// MCP tool result.
#[derive(Deserialize, Debug)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Deserialize, Debug)]
pub struct ToolContent {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub content_type: String,
    pub text: Option<String>,
}

/// Build a JSON-RPC POST request for a tool call.
fn build_tool_request(server_url: &str, tool_name: &str, arguments: serde_json::Value) -> ehttp::Request {
    let params = ToolCallParams {
        name: tool_name.to_string(),
        arguments,
    };

    let body = JsonRpcRequest {
        jsonrpc: "2.0",
        id: next_request_id(),
        method: "tools/call",
        params,
    };

    let body_bytes = serde_json::to_vec(&body).unwrap();

    let url = if server_url.is_empty() {
        "/api/mcp".to_string()
    } else {
        format!("{}/api/mcp", server_url)
    };
    let mut req = ehttp::Request::get(url);
    req.method = "POST".to_string();
    req.body = body_bytes;
    req.headers = ehttp::Headers::new(&[
        ("Content-Type", "application/json"),
        ("Accept", "application/json"),
    ]);
    req
}

/// Create an observe request.
pub fn create_observe_request(server_url: &str, match_id: u64, session_token: u64) -> ehttp::Request {
    build_tool_request(server_url, "observe", serde_json::json!({
        "match_id": match_id,
        "session_token": session_token,
    }))
}

/// Create a poll_events request.
pub fn create_poll_events_request(server_url: &str, match_id: u64, session_token: u64, cursor: u64) -> ehttp::Request {
    build_tool_request(server_url, "poll_events", serde_json::json!({
        "match_id": match_id,
        "session_token": session_token,
        "cursor": cursor,
    }))
}

/// Create a list_matches request.
pub fn create_list_matches_request(server_url: &str) -> ehttp::Request {
    build_tool_request(server_url, "list_matches", serde_json::json!({}))
}

/// Create a spectate_match request.
pub fn create_spectate_match_request(server_url: &str, match_id: u64) -> ehttp::Request {
    build_tool_request(server_url, "spectate_match", serde_json::json!({
        "match_id": match_id,
    }))
}

/// Create a leave_match request.
pub fn create_leave_match_request(server_url: &str, match_id: u64, session_token: u64) -> ehttp::Request {
    build_tool_request(server_url, "leave_match", serde_json::json!({
        "match_id": match_id,
        "session_token": session_token,
    }))
}

/// Parse a tool result from the response.
pub fn parse_tool_result<T: for<'de> Deserialize<'de>>(response: &ehttp::Response) -> Result<T, String> {
    let body = std::str::from_utf8(&response.bytes)
        .map_err(|e| format!("Invalid UTF-8: {}", e))?;

    let rpc_response: JsonRpcResponse<ToolResult> = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse JSON-RPC response: {} - body: {}", e, body))?;

    if let Some(error) = rpc_response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message));
    }

    let tool_result = rpc_response.result
        .ok_or_else(|| "Missing result in response".to_string())?;

    if tool_result.is_error {
        let error_text = tool_result.content.first()
            .and_then(|c| c.text.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("Unknown error");
        return Err(format!("Tool error: {}", error_text));
    }

    let text = tool_result.content.first()
        .and_then(|c| c.text.as_ref())
        .ok_or_else(|| "Missing content in tool result".to_string())?;

    serde_json::from_str(text)
        .map_err(|e| format!("Failed to parse tool result: {} - text: {}", e, text))
}
