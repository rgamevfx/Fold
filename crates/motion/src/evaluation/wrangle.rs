//! Embedded Rhai procedures. The host owns geometry, time and execution limits.
//! Prepared ASTs are process-local caches, never serialized project authority.
use super::{
    Geometry, GeometryData,
    attributes::{Domain, stable_id},
};
use crate::execution::Budget;
use fold_foundation::Time;
use rhai::{AST, Array, Dynamic, Engine, EvalAltResult, Scope};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
mod host;
use host::{empty, engine};
type Result<T> = std::result::Result<T, Box<EvalAltResult>>;
#[derive(Clone)]
struct Work {
    cancel: Arc<AtomicBool>,
    cost: Arc<AtomicU64>,
    allocated: Arc<AtomicUsize>,
    limit: usize,
    seed: u64,
    sequence: Arc<AtomicU64>,
}
impl Work {
    fn tick(&self) -> Result<()> {
        self.cost.fetch_add(1, Ordering::Relaxed);
        if self.cancel.load(Ordering::Relaxed) {
            Err("Wrangle cancelled".into())
        } else {
            Ok(())
        }
    }
    fn reserve(&self, n: usize) -> Result<()> {
        self.tick()?;
        if self
            .allocated
            .fetch_add(n, Ordering::Relaxed)
            .saturating_add(n)
            > self.limit
        {
            return Err("Wrangle geometry budget exhausted".into());
        }
        Ok(())
    }
    fn edit(&self, g: &Geometry) -> Result<()> {
        self.tick()?;
        if Arc::strong_count(&g.attributes.0) > 1 {
            self.reserve(g.attributes.0.iter().map(|a| a.values.len()).sum())?;
        }
        if let GeometryData::Mesh(m) = &g.data
            && (Arc::strong_count(m) > 1 || Arc::strong_count(&m.prepared.mesh) > 1)
        {
            self.reserve(m.vertex_ids.len() + m.face_ids.len())?;
        }
        if let GeometryData::Instances(c) = &g.data
            && Arc::strong_count(c) > 1
        {
            self.reserve(c.instances.len())?;
        }
        Ok(())
    }
    fn identity(&self) -> u64 {
        stable_id(self.seed, self.sequence.fetch_add(1, Ordering::Relaxed))
    }
}
static CACHE: OnceLock<Mutex<BTreeMap<String, AST>>> = OnceLock::new();
fn prepared(engine: &Engine, source: &str) -> std::result::Result<AST, String> {
    if source.len() > 65_536 {
        return Err("Wrangle source exceeds 64 KiB".into());
    }
    let cache = CACHE.get_or_init(Default::default);
    if let Some(ast) = cache
        .lock()
        .map_err(|_| "Wrangle cache unavailable")?
        .get(source)
    {
        return Ok(ast.clone());
    }
    let ast = engine.compile(source).map_err(|e| e.to_string())?;
    let mut cache = cache.lock().map_err(|_| "Wrangle cache unavailable")?;
    if cache.len() >= 32 {
        cache.clear()
    }
    cache.insert(source.into(), ast.clone());
    Ok(ast)
}
/// Rhai runs in a cancellable worker. Its callbacks count work; the caller owns
/// the shared graph budget and cancellation policy, including native host loops.
#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    source: &str,
    run_over: &str,
    parameters: &str,
    inputs: Vec<Arc<Geometry>>,
    time: Time,
    seed: u64,
    limit: usize,
    budget: &mut Budget<'_>,
) -> std::result::Result<Geometry, String> {
    budget.step()?;
    let work = Work {
        cancel: Arc::new(AtomicBool::new(false)),
        cost: Arc::new(AtomicU64::new(0)),
        allocated: Arc::new(AtomicUsize::new(0)),
        limit,
        seed,
        sequence: Arc::new(AtomicU64::new(0)),
    };
    std::thread::scope(|threads| {
        let w = work.clone();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        threads.spawn(move || {
            let result = run(source, run_over, parameters, inputs, time, w);
            let _ = send.send(result);
        });
        let mut charged = 0;
        let mut failure = None;
        loop {
            let result = receive.recv_timeout(Duration::from_millis(2));
            let cost = work.cost.load(Ordering::Relaxed);
            while charged < cost && failure.is_none() {
                if let Err(e) = budget.step() {
                    failure = Some(e);
                    work.cancel.store(true, Ordering::Relaxed)
                }
                charged += 1;
            }
            match result {
                Ok(result) => return failure.map_or(result, Err),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Wrangle worker stopped".into());
                }
                Err(_) => {
                    if failure.is_none()
                        && let Err(e) = budget.step()
                    {
                        failure = Some(e);
                        work.cancel.store(true, Ordering::Relaxed)
                    }
                }
            }
        }
    })
}
fn run(
    source: &str,
    run_over: &str,
    parameters: &str,
    inputs: Vec<Arc<Geometry>>,
    time: Time,
    work: Work,
) -> std::result::Result<Geometry, String> {
    if parameters.len() > 65_536 {
        return Err("Parameters exceed 64 KiB".into());
    }
    let parameters: serde_json::Value =
        serde_json::from_str(parameters).map_err(|e| format!("Parameters: {e}"))?;
    if !parameters.is_object() {
        return Err("Parameters must be a JSON object".into());
    }
    let engine = engine(work.clone());
    let ast = prepared(&engine, source)?;
    let domain = Domain::parse(run_over)?;
    let initial = inputs
        .first()
        .map(|g| g.as_ref().clone())
        .unwrap_or_else(|| empty(work.identity()));
    let count = if domain == Domain::Detail {
        1
    } else {
        initial.count(domain)?
    };
    let mut output = initial.clone();
    for index in 0..count {
        work.tick().map_err(|e| e.to_string())?;
        let mut scope = Scope::new();
        // Transfer the working geometry; retaining an extra output snapshot here
        // would force a full mesh copy on every element invocation.
        scope.push("geo", output);
        scope.push("input", initial.clone());
        scope.push(
            "inputs",
            inputs
                .iter()
                .map(|g| Dynamic::from(g.as_ref().clone()))
                .collect::<Array>(),
        );
        scope.push_constant("time", time.numerator() as f64 / time.denominator() as f64);
        scope.push_constant("time_numerator", time.numerator());
        scope.push_constant("time_denominator", time.denominator() as i64);
        scope.push_constant("index", index as i64);
        scope.push_constant(
            "id",
            if domain == Domain::Detail {
                seed_i64(work.seed)
            } else {
                initial.element_id(domain, index)? as i64
            },
        );
        scope.push_constant(
            "params",
            rhai::serde::to_dynamic(&parameters).map_err(|e| e.to_string())?,
        );
        output = engine
            .eval_ast_with_scope::<Geometry>(&mut scope, &ast)
            .map_err(|e| format!("Wrangle {run_over} element {index}: {e}"))?;
    }
    output.id = stable_id(work.seed, output.id);
    validate(&output, work.limit)?;
    Ok(output)
}
fn seed_i64(seed: u64) -> i64 {
    seed as i64
}
fn validate(g: &Geometry, limit: usize) -> std::result::Result<(), String> {
    let count = g
        .count(Domain::Point)
        .or_else(|_| g.count(Domain::Instance))?;
    if count > limit {
        return Err("Wrangle output exceeds geometry budget".into());
    }
    if let GeometryData::Mesh(m) = &g.data {
        let p = &m.prepared.mesh;
        if p.positions.len() != m.vertex_ids.len()
            || p.normals.len() != p.positions.len()
            || p.indices.len() != m.face_ids.len() * 3
        {
            return Err("Invalid output topology".into());
        }
        if p.positions
            .iter()
            .flatten()
            .any(|x| !x.is_finite() || x.abs() > 1e6)
            || p.indices.iter().any(|i| *i as usize >= count)
        {
            return Err("Invalid output positions/indices".into());
        }
    }
    for a in g.attributes.0.iter() {
        if a.values.len() != g.count(a.domain)? {
            return Err("Invalid output attribute count".into());
        }
        for value in &a.values {
            value.validate()?
        }
    }
    Ok(())
}

/// Function signatures are generated from the actual registered Rhai engine.
pub fn reference() -> Vec<String> {
    let work = Work {
        cancel: Arc::new(AtomicBool::new(false)),
        cost: Arc::new(AtomicU64::new(0)),
        allocated: Arc::new(AtomicUsize::new(0)),
        limit: 100_000,
        seed: 0,
        sequence: Arc::new(AtomicU64::new(0)),
    };
    engine(work).gen_fn_signatures(false)
}
pub fn check_source(source: &str) -> std::result::Result<(), String> {
    let work = Work {
        cancel: Arc::new(AtomicBool::new(false)),
        cost: Arc::new(AtomicU64::new(0)),
        allocated: Arc::new(AtomicUsize::new(0)),
        limit: 100_000,
        seed: 0,
        sequence: Arc::new(AtomicU64::new(0)),
    };
    prepared(&engine(work), source).map(|_| ())
}
