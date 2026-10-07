//! Disposable, bounded runtime Rust compiler. No Cargo, dependencies, build scripts
//! or source includes. Compiled artifacts never become authoritative project data.
use super::source::{self, Channel};
use fold_media::Cancel;
use std::os::unix::process::CommandExt;
use std::{
    collections::VecDeque,
    process::{Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use wasmtime::{Config, Engine, Module, PoolingAllocationConfig};
pub struct Program {
    pub engine: Engine,
    pub module: Module,
    pub element_local: bool,
    pub attributes: Vec<Channel>,
}
static CACHE: OnceLock<Mutex<VecDeque<(String, Arc<Program>)>>> = OnceLock::new();
// The compiler gate also deduplicates simultaneous cold requests from render workers.
static COMPILE: Mutex<VecDeque<(std::ffi::OsString, String, Module)>> = Mutex::new(VecDeque::new());
pub fn compile(
    source: &str,
    channels: &[Channel],
    cancel: &Cancel,
) -> Result<Arc<Program>, String> {
    cancel.check()?;
    if channels.len() > 64 {
        return Err("wrangle supports at most 64 channels".into());
    }
    if source.len() > 32768 {
        return Err("wrangle source exceeds 32 KiB".into());
    }
    let compiler = std::env::var_os("FOLD_RUSTC").unwrap_or_else(|| "rustc".into());
    let cache_key = format!(
        "{:?}\n{}\n{}",
        compiler,
        serde_json::to_string(channels).map_err(|e| e.to_string())?,
        source
    );
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, p)) = cache
        .lock()
        .map_err(|_| "wrangle cache poisoned")?
        .iter()
        .find(|(k, _)| k == &cache_key)
    {
        return Ok(p.clone());
    }
    let (discovered, attributes) = source::bindings(source)?;
    for c in discovered {
        if !channels.iter().any(|p| p.id == c.id && p.kind == c.kind) {
            return Err(format!("missing channel {}; use Create Controls", c.id));
        }
    }
    let code = source::module(source, channels, &attributes);
    let module = compile_module(&code, cancel)?;
    let engine = engine()?.clone();
    let p = Arc::new(Program {
        engine,
        module,
        element_local: source::element_local(source),
        attributes,
    });
    let mut cache = cache.lock().map_err(|_| "wrangle cache poisoned")?;
    while cache.len() >= 8 {
        cache.pop_front();
    }
    cache.push_back((cache_key, p.clone()));
    Ok(p)
}

/// Shared bounded compiler for trusted wrappers around validated guest source.
pub(crate) fn compile_module(code: &str, cancel: &Cancel) -> Result<Module, String> {
    // Serialize expensive compiler jobs; waiting remains cancellable.
    let mut modules = loop {
        cancel.check()?;
        if let Ok(g) = COMPILE.try_lock() {
            break g;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let rustc = std::env::var_os("FOLD_RUSTC").unwrap_or_else(|| "rustc".into());
    if let Some((_, _, module)) = modules
        .iter()
        .find(|(compiler, source, _)| compiler == &rustc && source == code)
    {
        return Ok(module.clone());
    }
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("wrangle.rs"), code.as_bytes()).map_err(|e| e.to_string())?;
    let stderr =
        std::fs::File::create(dir.path().join("diagnostics.jsonl")).map_err(|e| e.to_string())?;
    let mut child=Command::new("prlimit").args(["--as=4294967296","--cpu=20","--fsize=8388608","--nofile=128","--"])
        .arg(&rustc).args(["--edition=2024","--crate-type=cdylib","--target=wasm32-unknown-unknown","--crate-name=fold_wrangle","-Copt-level=1","-Cpanic=abort","-Clink-arg=--max-memory=16777216","-Clink-arg=--no-stack-first","--error-format=json","-Aunused","wrangle.rs","-o","wrangle.wasm"])
        .process_group(0).current_dir(dir.path()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(stderr).spawn()
        .map_err(|e|format!("Rust Wrangle compiler unavailable: {e}. Install rustc, wasm32-unknown-unknown and prlimit; FOLD_RUSTC may select rustc."))?;
    let start = Instant::now();
    let status = loop {
        if let Err(e) = cancel.check() {
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .status();
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .status();
            let _ = child.kill();
            let _ = child.wait();
            return Err("Rust compilation exceeded 30 seconds".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                let _ = Command::new("kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    };
    if !status.success() {
        let bytes =
            std::fs::read(dir.path().join("diagnostics.jsonl")).map_err(|e| e.to_string())?;
        let diagnostics = String::from_utf8_lossy(&bytes[..bytes.len().min(65536)]);
        let offset = code
            .lines()
            .position(|line| line == "// FOLD_USER_SOURCE")
            .unwrap_or(0)
            + 1;
        let errors:Vec<serde_json::Value>=diagnostics.lines().filter_map(|line|serde_json::from_str::<serde_json::Value>(line).ok()).filter(|d|d["level"]=="error").map(|d|{
            let spans:Vec<_>=d["spans"].as_array().into_iter().flatten().filter(|s|s["is_primary"]==true).map(|s|serde_json::json!({"line":s["line_start"].as_u64().unwrap_or(0).saturating_sub(offset as u64),"column":s["column_start"],"label":s["label"]})).collect();
            serde_json::json!({"message":d["message"],"code":d["code"]["code"],"spans":spans})
        }).collect();
        return Err(serde_json::json!({"kind":"rust_compile","message":"Rust compilation failed","diagnostics":errors,"detail":if errors.is_empty(){diagnostics.as_ref()}else{""}}).to_string());
    }
    cancel.check()?;
    let wasm = std::fs::read(dir.path().join("wrangle.wasm")).map_err(|e| e.to_string())?;
    if wasm.len() > 8 * 1024 * 1024 {
        return Err("compiled wrangle exceeds 8 MiB".into());
    }
    let engine = engine()?.clone();
    let module = Module::new(&engine, wasm).map_err(|e| e.to_string())?;
    if modules.len() >= 8 {
        modules.pop_front();
    }
    modules.push_back((rustc, code.into(), module.clone()));
    Ok(module)
}

// Share a bounded pool across compiled programs. Each dropped Store returns a slot;
// Wasmtime resets its memory, globals and tables before the next instantiation.
pub(crate) fn engine() -> Result<&'static Engine, String> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut pool = PoolingAllocationConfig::default();
            pool.total_memories(32)
                .total_tables(64)
                .total_core_instances(32)
                .max_memories_per_module(1)
                .max_tables_per_module(2)
                .max_memory_size(16 * 1024 * 1024)
                .linear_memory_keep_resident(64 * 1024)
                .table_keep_resident(4096);
            let mut config = Config::new();
            config
                .consume_fuel(true)
                .max_wasm_stack(256 * 1024)
                .memory_reservation(16 * 1024 * 1024)
                .allocation_strategy(pool);
            Engine::new(&config).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}
