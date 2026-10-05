use super::*;
use std::io::{BufReader, Write};
pub fn sessions() -> Result<Value, String> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir(wire::runtime()?)
        .map_err(|e| e.to_string())?
        .take(256)
    {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if let Ok(state) = call(&id, "state", json!({}))
                && state["ok"] == true
            {
                result.push(json!({"session":id,"state":state["result"]}));
            }
        }
    }
    Ok(json!(result))
}
#[cfg(unix)]
pub fn call(session: &str, method: &str, params: Value) -> Result<Value, String> {
    // Session IDs are UUIDs, never arbitrary paths.
    let _: fold_foundation::ObjectId =
        serde_json::from_value(json!(session)).map_err(|_| "invalid session ID")?;
    let mut stream =
        std::os::unix::net::UnixStream::connect(wire::runtime()?.join(session).join("ipc"))
            .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(35)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut bytes = serde_json::to_vec(&Request {
        version: VERSION,
        method: method.into(),
        params,
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() + 1 > MAX_REQUEST {
        return Err("request exceeds 1 MiB".into());
    }
    bytes.push(b'\n');
    stream.write_all(&bytes).map_err(|e| e.to_string())?;
    let response = wire::line(&mut BufReader::new(stream), MAX_RESPONSE)?;
    serde_json::from_slice(&response).map_err(|e| e.to_string())
}
#[cfg(not(unix))]
pub fn call(_: &str, _: &str, _: Value) -> Result<Value, String> {
    Err("live agent IPC currently requires Unix".into())
}
