//! Author-time Rust execution returns data only. No imports or live project access.
use super::Blueprint;
use fold_media::Cancel;
use std::sync::{Mutex, OnceLock};
use syn::visit::{self, Visit};
use wasmtime::{Linker, Store, StoreLimitsBuilder};

pub const SDK: &str = include_str!("sdk.rs");
static CACHE: OnceLock<Mutex<Vec<(String, Blueprint)>>> = OnceLock::new();

#[derive(Default)]
struct Validate {
    error: Option<String>,
}
impl<'a> Visit<'a> for Validate {
    fn visit_attribute(&mut self, _: &'a syn::Attribute) {
        self.error = Some("Rust asset attributes are unavailable".into());
    }
    fn visit_macro(&mut self, _: &'a syn::Macro) {
        self.error = Some(
            "Rust asset macros are unavailable; use SDK helpers and ordinary functions".into(),
        );
    }
    fn visit_item(&mut self, item: &'a syn::Item) {
        if matches!(item, syn::Item::Fn(_)) {
            visit::visit_item(self, item)
        } else {
            self.error=Some("Rust assets support local functions and expressions, not imports/modules/static state".into());
        }
    }
}

pub fn compile(source: &str, cancel: &Cancel) -> Result<Blueprint, String> {
    cancel.check()?;
    if source.len() > 65536
        || source
            .bytes()
            .filter(|b| matches!(b, b'(' | b'[' | b'{'))
            .count()
            > 1024
    {
        return Err("Rust asset source exceeds 64 KiB or 1024 opening delimiters".into());
    }
    let block: syn::Block = syn::parse_str(&format!("{{\n{source}\n}}"))
        .map_err(|e| format!("Rust asset syntax: {e}"))?;
    let mut validation = Validate::default();
    validation.visit_block(&block);
    if let Some(error) = validation.error {
        return Err(error);
    }
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, result)) = cache
        .lock()
        .map_err(|_| "asset cache poisoned")?
        .iter()
        .find(|(s, _)| s == source)
    {
        return Ok(result.clone());
    }
    let code = format!(
        "{SDK}\n#[unsafe(no_mangle)]\npub extern \"C\" fn build()->u64 {{\nlet asset: Asset = {{\n// FOLD_USER_SOURCE\n{source}\n}};\nlet bytes=asset.finish().into_bytes().into_boxed_slice();\nlet bytes=Box::leak(bytes);\n((bytes.as_ptr() as u64)<<32) | bytes.len() as u64\n}}\n"
    );
    let module = crate::wrangle::compiler::compile_module(&code, cancel)?;
    cancel.check()?;
    let engine = crate::wrangle::compiler::engine()?;
    let limits = StoreLimitsBuilder::new()
        .memory_size(16 * 1024 * 1024)
        .instances(1)
        .memories(1)
        .tables(1)
        .build();
    let mut store = Store::new(engine, limits);
    store.limiter(|limits| limits);
    store.set_fuel(10_000_000).map_err(|e| e.to_string())?;
    let linker = Linker::new(engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| format!("Rust asset instantiation: {e}"))?;
    let build = instance
        .get_typed_func::<(), u64>(&mut store, "build")
        .map_err(|e| e.to_string())?;
    let result = build
        .call(&mut store, ())
        .map_err(|e| format!("Rust asset execution: {e}"))?;
    cancel.check()?;
    let length = (result & 0xffff_ffff) as usize;
    if length > 1024 * 1024 {
        return Err("Rust asset output exceeds 1 MiB".into());
    }
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or("asset memory missing")?;
    let mut bytes = vec![0; length];
    memory
        .read(&store, (result >> 32) as usize, &mut bytes)
        .map_err(|e| e.to_string())?;
    let result: Blueprint =
        serde_json::from_slice(&bytes).map_err(|e| format!("Rust asset output: {e}"))?;
    result.validate()?;
    let mut cache = cache.lock().map_err(|_| "asset cache poisoned")?;
    if cache.len() >= 8 {
        cache.remove(0);
    }
    cache.push((source.into(), result.clone()));
    Ok(result)
}
