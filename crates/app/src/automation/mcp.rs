//! MCP stdio adapter for the 2024-11-05/2025-03-26/2025-06-18/2025-11-25 lifecycle.
//! Fold sessions are explicit tool arguments, independent of protocol connections.
use super::*;
use std::io::{BufRead, Write};
pub fn tools() -> Value {
    let mut methods = super::schema::methods();
    for method in methods.as_array_mut().unwrap() {
        let name = method["name"].as_str().unwrap().to_owned();
        if name != "sessions" {
            method["inputSchema"]["properties"]["session"] = json!({"type":"string"});
            method["inputSchema"]["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("session"));
        }
        method["name"] = json!(format!("fold_{name}"));
    }
    methods
}
fn images(value: &mut Value, out: &mut Vec<Value>) {
    match value {
        Value::Object(o) => {
            if let Some(image) = o.remove("image") {
                if image["mimeType"] == "image/png" && image["data"].is_string() {
                    out.push(json!({"type":"image","mimeType":"image/png","data":image["data"]}));
                } else {
                    o.insert("image".into(), image);
                }
            }
            for v in o.values_mut() {
                images(v, out);
            }
        }
        Value::Array(a) => {
            for v in a {
                images(v, out)
            }
        }
        _ => {}
    }
}
pub fn tool_call(params: &Value) -> Result<Value, String> {
    let name = params["name"].as_str().ok_or("missing tool name")?;
    if !tools()
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == name)
    {
        return Err("unknown tool".into());
    }
    let method = name.strip_prefix("fold_").ok_or("unknown tool")?;
    let mut args = params.get("arguments").cloned().unwrap_or(json!({}));
    let result = if method == "sessions" {
        client::sessions().map(success)
    } else {
        let session: String = required(&args, "session")?;
        args.as_object_mut()
            .ok_or("arguments must be an object")?
            .remove("session");
        client::call(&session, method, args)
    };
    let mut value = result.unwrap_or_else(error);
    let is_error = value["ok"] == false;
    let mut content = Vec::new();
    images(&mut value, &mut content);
    content.insert(0, json!({"type":"text","text":value.to_string()}));
    Ok(json!({"content":content,"isError":is_error}))
}
pub fn serve(reader: &mut impl BufRead, writer: &mut impl Write) -> Result<(), String> {
    let mut initialized = false;
    loop {
        if reader.fill_buf().map_err(|e| e.to_string())?.is_empty() {
            return Ok(());
        }
        let line = wire::line(reader, MAX_REQUEST)?;
        let request: Value = match serde_json::from_slice(&line) {
            Ok(v) => v,
            Err(_) => {
                writeln!(writer,"{}",json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON"}})).map_err(|e|e.to_string())?;
                writer.flush().map_err(|e| e.to_string())?;
                continue;
            }
        };
        let id = request.get("id").cloned();
        if id.is_none() {
            continue;
        }
        let method = request["method"].as_str().unwrap_or("");
        let result = if request["jsonrpc"] != "2.0" {
            Err((-32600, "Invalid JSON-RPC request".to_owned()))
        } else {
            match method {
                "initialize" => {
                    let requested = request["params"]["protocolVersion"].as_str().unwrap_or("");
                    let version = if ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"]
                        .contains(&requested)
                    {
                        requested
                    } else {
                        "2025-11-25"
                    };
                    initialized = true;
                    Ok(
                        json!({"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"fold","version":env!("CARGO_PKG_VERSION")},"instructions":"Discover a running Fold session, inspect state and relevant catalog entries. Edits and images return jobs; poll completion and inspect outcome. Preserve user edits with revision checks. Use the same request_id only for identical retries."}),
                    )
                }
                "ping" => Ok(json!({})),
                _ if !initialized => Err((-32002, "Initialize first".into())),
                "tools/list" => Ok(json!({"tools":tools()})),
                "tools/call" => tool_call(&request["params"]).map_err(|e| (-32602, e)),
                _ => Err((-32601, "Method not found".into())),
            }
        };
        let response = match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err((code, message)) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
            }
        };
        writeln!(writer, "{response}").map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
    }
}
