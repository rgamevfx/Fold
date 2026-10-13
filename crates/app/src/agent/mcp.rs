//! Stdio MCP adapter. Fold arguments pass unchanged through the local transport.
use super::{local, retry, wire};
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use std::{
    io::{self, BufRead, Read, Write},
    path::Path,
};
const VERSION: &str = "2025-11-25";
const MAX_LINE: usize = retry::MAX_REQUEST_BYTES + 65536;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rpc {
    jsonrpc: String,
    #[serde(default)]
    id: Value,
    method: String,
    params: Option<Box<RawValue>>,
}
#[derive(Deserialize)]
struct Call {
    name: String,
    arguments: Box<RawValue>,
}
#[derive(Default)]
struct Adapter {
    initialized: bool,
    ready: bool,
}
fn error(id: Value, code: i32, message: impl Into<String>) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message.into()}})
}
impl Adapter {
    fn message(
        &mut self,
        bytes: &[u8],
        request: &mut impl FnMut(&[u8]) -> io::Result<Vec<u8>>,
    ) -> Option<Value> {
        let header: Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(_) => return Some(error(Value::Null, -32700, "Invalid JSON")),
        };
        let rpc: Rpc = match serde_json::from_slice(bytes) {
            Ok(rpc) => rpc,
            Err(_) => return Some(error(Value::Null, -32600, "Invalid request")),
        };
        let notification = header.get("id").is_none();
        if rpc.jsonrpc != "2.0"
            || !(notification || rpc.id.is_string() || rpc.id.is_i64() || rpc.id.is_u64())
        {
            return Some(error(Value::Null, -32600, "Invalid JSON-RPC envelope"));
        }
        if notification {
            if rpc.method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            // Jobs have explicit guarded cancellation; notifications grant no write authority.
            return None;
        }
        let params: Value = rpc
            .params
            .as_ref()
            .and_then(|p| serde_json::from_str(p.get()).ok())
            .unwrap_or(Value::Null);
        let result = match rpc.method.as_str() {
            "ping" => json!({}),
            "initialize" if !self.initialized => {
                if !params["protocolVersion"].is_string()
                    || !params["capabilities"].is_object()
                    || !params["clientInfo"]["name"].is_string()
                    || !params["clientInfo"]["version"].is_string()
                {
                    return Some(error(rpc.id, -32602, "Invalid initialize parameters"));
                }
                self.initialized = true;
                json!({"protocolVersion":VERSION,"capabilities":{"tools":{}},"serverInfo":{"name":"fold","version":env!("CARGO_PKG_VERSION")},"instructions":"Use fold_request with Fold wire version 2. Jobs return handles; inspect the exact candidate before Apply. Retry identical arguments with the same Fold request ID. MCP request IDs are independent."})
            }
            _ if !self.ready => {
                return Some(error(
                    rpc.id,
                    -32002,
                    "Initialize and send notifications/initialized first",
                ));
            }
            "tools/list" => {
                if params.get("cursor").is_some() {
                    return Some(error(rpc.id, -32602, "This tool list has no cursor"));
                }
                json!({"tools":[{"name":"fold_request","description":"Discover capabilities, inspect documents, propose native or project edits, inspect and apply candidates, observe frames, save project copies or manage jobs through Fold's versioned protocol. Apply changes the open project; save writes a new file. Frame observations require the discovered GPU capability. Use discovery for provider schemas and limits.","inputSchema":wire::schema(),"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":false}}]})
            }
            "tools/call" => {
                if params.get("task").is_some() {
                    return Some(error(
                        rpc.id,
                        -32602,
                        "MCP task execution is not supported; use Fold jobs",
                    ));
                }
                let call = rpc
                    .params
                    .as_ref()
                    .and_then(|p| serde_json::from_str::<Call>(p.get()).ok());
                let Some(call) = call.filter(|call| call.name == "fold_request") else {
                    return Some(error(rpc.id, -32602, "Expected fold_request and arguments"));
                };
                match request(call.arguments.get().as_bytes()) {
                    Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                        Ok(value) => {
                            json!({"content":[{"type":"text","text":String::from_utf8_lossy(&bytes)}],"isError":value.get("error").is_some(),"structuredContent":value})
                        }
                        Err(_) => return Some(error(rpc.id, -32603, "Invalid Fold response")),
                    },
                    Err(e) => {
                        json!({"content":[{"type":"text","text":format!("Local transport failed: {e}. A lost response may follow a committed write; retry identical Fold arguments.")}],"isError":true})
                    }
                }
            }
            _ => return Some(error(rpc.id, -32601, "Unsupported method")),
        };
        Some(json!({"jsonrpc":"2.0","id":rpc.id,"result":result}))
    }
}
pub fn serve(path: &Path, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut adapter = Adapter::default();
    loop {
        let mut line = Vec::new();
        let count = input
            .by_ref()
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(());
        }
        if line.len() > MAX_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "MCP message exceeds limit",
            ));
        }
        if let Some(response) = adapter.message(&line, &mut |bytes| local::request(path, bytes)) {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_schema_and_raw_arguments_share_the_wire_contract() {
        let mut adapter = Adapter::default();
        let mut no_call = |_: &[u8]| -> io::Result<Vec<u8>> { panic!("not a tool call") };
        assert_eq!(
            adapter
                .message(
                    br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
                    &mut no_call
                )
                .unwrap()["error"]["code"],
            -32002
        );
        let init = br#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#;
        assert_eq!(
            adapter.message(init, &mut no_call).unwrap()["result"]["protocolVersion"],
            VERSION
        );
        assert!(
            adapter
                .message(
                    br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                    &mut no_call
                )
                .is_none()
        );
        assert_eq!(
            adapter
                .message(
                    br#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#,
                    &mut no_call
                )
                .unwrap()["result"]["tools"][0]["inputSchema"],
            wire::schema()
        );
        let response = adapter.message(br#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"fold_request","arguments":{ "version":1, "command":{"method":"discover"} }}}"#, &mut |bytes| {
            assert_eq!(bytes, br#"{ "version":1, "command":{"method":"discover"} }"#);
            Ok(br#"{"error":{"code":"version"}}"#.to_vec())
        }).unwrap();
        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["structuredContent"]["error"]["code"],
            "version"
        );
        assert!(adapter.message(br#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"fold_request","arguments":{}}}"#, &mut no_call).is_none());
    }
    #[test]
    fn malformed_and_oversized_messages_never_reach_fold() {
        let mut adapter = Adapter::default();
        let mut reject = |_: &[u8]| -> io::Result<Vec<u8>> { panic!("unexpected dispatch") };
        assert_eq!(
            adapter.message(b"{", &mut reject).unwrap()["error"]["code"],
            -32700
        );
        assert_eq!(
            adapter
                .message(
                    br#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
                    &mut reject
                )
                .unwrap()["error"]["code"],
            -32600
        );
        assert_eq!(
            adapter
                .message(
                    br#"{"jsonrpc":"2.0","id":1,"id":2,"method":"ping"}"#,
                    &mut reject
                )
                .unwrap()["error"]["code"],
            -32600
        );
        let input = vec![b' '; MAX_LINE + 1];
        let mut output = vec![];
        assert_eq!(
            serve(Path::new("/no/socket"), input.as_slice(), &mut output)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(output.is_empty());
    }
}
