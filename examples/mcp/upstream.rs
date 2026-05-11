//! Upstream MCP server connection params. Stub: in production this is a
//! `mcp_rs::stdio_client(params)` returning a typed `ClientSession`.

#![allow(unreachable_pub, clippy::todo, clippy::unimplemented, dead_code)]

pub struct UpstreamParams {
    pub command: String,
    pub args: Vec<String>,
}

pub fn upstream_params() -> UpstreamParams {
    todo!()
}

pub struct ClientSession;

impl ClientSession {
    pub async fn initialize(&self) {}

    pub async fn list_tools(&self) -> Vec<String> {
        todo!()
    }

    pub async fn call_tool(&self, _name: &str, _args: serde_json::Value) -> serde_json::Value {
        todo!()
    }
}
