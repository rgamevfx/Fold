//! Motion-owned Rust source, compiler and bounded WebAssembly execution.
pub mod compiler;
pub mod source;
use crate::{
    evaluation::{Value, output},
    fields::{Datum, Kind},
    geometry::Domain,
    graph::{
        Node,
        definition::{Definition, Socket},
    },
    nodes,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wasmtime::{Linker, Store, StoreLimitsBuilder};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub source: String,
    #[serde(default = "version")]
    pub abi: u32,
    pub domain: Domain,
    pub geometry_kind: Kind,
    #[serde(default)]
    pub channels: Vec<source::Channel>,
}
fn version() -> u32 {
    1
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            source: "let amplitude = chf(\"amplitude\");\nlet phase = tau * time.seconds;\nposition[1] += amplitude * phase.sin();".into(),
            abi: 1,
            domain: Domain::Points,
            geometry_kind: Kind::Points,
            channels: vec![source::Channel {
                id: "amplitude".into(),
                kind: Kind::Scalar,
                default: Datum::Scalar(40.),
            }],
        }
    }
}
pub fn sync_controls(n: &mut Node) -> Result<(), String> {
    let mut s = n.settings::<Settings>()?;
    for c in source::discover(&s.source)? {
        if let Some(old) = s.channels.iter().find(|old| old.id == c.id) {
            if old.kind != c.kind {
                return Err(format!(
                    "channel {} changed type; migrate its bindings explicitly",
                    c.id
                ));
            }
        } else {
            s.channels.push(c);
        }
    }
    for c in &s.channels {
        n.inputs
            .entry(c.id.clone())
            .or_insert_with(|| crate::graph::Input::Value(c.default.clone()));
    }
    // Preserve unknown settings fields.
    fold_platform::authoring::merge(
        &mut n.settings,
        &serde_json::to_value(s).map_err(|e| e.to_string())?,
    );
    Ok(())
}
fn signature(n: &Node) -> Result<(Vec<Socket>, Vec<(String, Kind)>), String> {
    let s = n.settings::<Settings>()?;
    if s.abi != 1
        || !matches!(s.geometry_kind, Kind::Points | Kind::Content)
        || s.channels.len() > 64
        || s.source.len() > 32768
    {
        return Err("unsupported wrangle ABI, geometry kind or source/channel limit".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut inputs = vec![Socket::required("content", s.geometry_kind)];
    for c in s.channels {
        if c.id.is_empty()
            || c.id == "content"
            || c.id.len() > 80
            || !seen.insert(c.id.clone())
            || c.default.kind() != c.kind
            || !matches!(
                c.kind,
                Kind::Scalar | Kind::Vector | Kind::Color | Kind::Bool
            )
        {
            return Err("invalid wrangle channel".into());
        }
        c.default.validate()?;
        inputs.push(Socket {
            id: c.id,
            kind: c.kind,
            default: Some(c.default),
            unit: "",
        });
    }
    Ok((inputs, vec![("content".into(), s.geometry_kind)]))
}
pub fn definition() -> Definition {
    let mut d = nodes::definition(
        "fold.motion.wrangle",
        "Rust Wrangle",
        "Code",
        vec![
            Socket::required("content", Kind::Points),
            Socket::value("amplitude", Datum::Scalar(40.), ""),
        ],
        vec![("content", Kind::Points)],
        |e, n| {
            let settings = n.settings::<Settings>()?;
            let program =
                compiler::compile(&settings.source, &settings.channels, e.budget.cancel())?;
            let source = e.content(n, "content")?;
            let attributes = &program.attributes;
            let channels = settings
                .channels
                .iter()
                .map(|c| e.field(n, &c.id))
                .collect::<Result<Vec<_>, _>>()?;
            let linker = Linker::new(&program.engine);
            let instance_pre = linker
                .instantiate_pre(&program.module)
                .map_err(|e| e.to_string())?;
            let mut uniform: Vec<Option<Datum>> = vec![None; channels.len()];
            let attribute_base = 32 + channels.len() * 4;
            let mut values = vec![0.; attribute_base + attributes.len() * 5];
            let mut result = Vec::with_capacity(source.len());
            let mut remaining = 10_000_000u64;
            let mut guest = None;
            for (index, item) in source.iter().enumerate() {
                e.budget.spend()?;
                if !item.matches(settings.domain) {
                    result.push(item.clone());
                    continue;
                }
                let sample = crate::nodes::attributes::sample(item, index, source.len());
                values.fill(0.);
                values[0] = sample.position[0];
                values[1] = sample.position[1];
                values[2..6].copy_from_slice(&item.fill.unwrap_or([1.; 4]));
                values[6] = item.opacity;
                values[7] = e.time.numerator() as f64 / e.time.denominator() as f64;
                if let Some(step) = &e.step_context {
                    values[8] = step.delta.numerator() as f64 / step.delta.denominator() as f64;
                }
                for (i, ch) in channels.iter().enumerate() {
                    let data = match &uniform[i] {
                        Some(value) => value.clone(),
                        None => {
                            let value = ch.sample(&sample, &mut e.budget)?;
                            if !ch.varying {
                                uniform[i] = Some(value.clone());
                            }
                            value
                        }
                    };
                    values[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&components(&data)?);
                }
                for (i, a) in attributes.iter().enumerate() {
                    let value = item.attributes.get(&a.id).unwrap_or(&a.default);
                    if value.kind() != a.kind {
                        return Err(format!("attribute {} has incompatible type", a.id));
                    }
                    let data = components(value)?;
                    values[attribute_base + i * 4..attribute_base + i * 4 + data.len()]
                        .copy_from_slice(&data);
                }
                let exact = [
                    e.time.numerator(),
                    e.time.denominator() as i64,
                    index as i64,
                    source.len() as i64,
                    item.id as i64,
                ];
                let fuel = remaining.min(100_000);
                if guest.is_none() {
                    let limits = StoreLimitsBuilder::new()
                        .memory_size(16 * 1024 * 1024)
                        .instances(1)
                        .tables(2)
                        .table_elements(4096)
                        .build();
                    let mut store = Store::new(&program.engine, limits);
                    store.limiter(|s| s);
                    store.set_fuel(100_000).map_err(|e| e.to_string())?;
                    let instance = instance_pre
                        .instantiate(&mut store)
                        .map_err(|e| e.to_string())?;
                    let memory = instance
                        .get_memory(&mut store, "memory")
                        .ok_or("missing Wrangle memory")?;
                    let values_ptr = instance
                        .get_typed_func::<(), u32>(&mut store, "values_ptr")
                        .map_err(|e| e.to_string())?
                        .call(&mut store, ())
                        .map_err(|e| e.to_string())? as usize;
                    let exact_ptr = instance
                        .get_typed_func::<(), u32>(&mut store, "exact_ptr")
                        .map_err(|e| e.to_string())?
                        .call(&mut store, ())
                        .map_err(|e| e.to_string())? as usize;
                    let run = instance
                        .get_typed_func::<(), ()>(&mut store, "run")
                        .map_err(|e| e.to_string())?;
                    guest = Some((store, memory, values_ptr, exact_ptr, run));
                }
                let (store, memory, values_ptr, exact_ptr, run) = guest.as_mut().unwrap();
                let (values_ptr, exact_ptr) = (*values_ptr, *exact_ptr);
                store.set_fuel(fuel).map_err(|e| e.to_string())?;
                let bytes = memory.data_mut(&mut *store);
                for (slot, value) in bytes
                    .get_mut(values_ptr..values_ptr + values.len() * 8)
                    .ok_or("invalid Wrangle value buffer")?
                    .chunks_exact_mut(8)
                    .zip(&values)
                {
                    slot.copy_from_slice(&value.to_le_bytes());
                }
                for (slot, value) in bytes
                    .get_mut(exact_ptr..exact_ptr + 40)
                    .ok_or("invalid Wrangle integer buffer")?
                    .chunks_exact_mut(8)
                    .zip(exact)
                {
                    slot.copy_from_slice(&value.to_le_bytes());
                }
                run.call(&mut *store, ())
                    .map_err(|e| format!("Wrangle element {index}: {e:#}"))?;
                remaining =
                    remaining.saturating_sub(fuel - store.get_fuel().map_err(|e| e.to_string())?);
                for (value, bytes) in values.iter_mut().zip(
                    memory
                        .data(&*store)
                        .get(values_ptr..values_ptr + (attribute_base + attributes.len() * 5) * 8)
                        .ok_or("invalid Wrangle output buffer")?
                        .chunks_exact(8),
                ) {
                    *value = f64::from_le_bytes(bytes.try_into().unwrap());
                }
                let v = &values;
                if !v.iter().all(|v| v.is_finite()) {
                    return Err(format!("Wrangle element {index} returned nonfinite values"));
                }
                let mut item = item.clone();
                item.transform[4] = v[0];
                item.transform[5] = v[1];
                let color: [f64; 4] = v[2..6].try_into().unwrap();
                if item.fill.is_some() || color != [1.; 4] {
                    item.fill = Some(color);
                }
                item.opacity = v[6];
                for (i, a) in attributes.iter().enumerate() {
                    if v[attribute_base + attributes.len() * 4 + i] != 0.0 {
                        if item.attributes.len() >= 64 && !item.attributes.contains_key(&a.id) {
                            return Err("element attribute limit is 64".into());
                        }
                        let data = &v[attribute_base + i * 4..attribute_base + i * 4 + 4];
                        let value = match a.kind {
                            Kind::Scalar => Datum::Scalar(data[0]),
                            Kind::Vector => Datum::Vector([data[0], data[1]]),
                            Kind::Color => Datum::Color(data.try_into().unwrap()),
                            Kind::Bool => Datum::Bool(data[0] != 0.),
                            _ => unreachable!(),
                        };
                        item.attributes.insert(a.id.clone(), value);
                    }
                }
                result.push(item);
                if !program.element_local {
                    // Arbitrary Rust may retain allocator/memory state: discard it.
                    guest = None;
                }
            }
            let result = Arc::new(result);
            Ok(output(
                "content",
                if settings.geometry_kind == Kind::Points {
                    Value::Points(result)
                } else {
                    Value::Content(result)
                },
            ))
        },
    );
    d.defaults = || serde_json::to_value(Settings::default()).unwrap();
    d.signature = Some(signature);
    d.validate = |n| signature(n).map(|_| ());
    d
}

fn components(value: &Datum) -> Result<[f64; 4], String> {
    Ok(match value {
        Datum::Scalar(v) => [*v, 0., 0., 0.],
        Datum::Vector(v) => [v[0], v[1], 0., 0.],
        Datum::Color(v) => *v,
        Datum::Bool(v) => [if *v { 1. } else { 0. }, 0., 0., 0.],
        _ => return Err("unsupported wrangle value type".into()),
    })
}
