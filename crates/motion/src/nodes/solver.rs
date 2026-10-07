//! A fixed-step state region whose update is an ordinary editable node group.
use super::*;
use crate::{
    Motion,
    evaluation::{Value, output},
    fields::Field,
    geometry::Content,
};
use fold_foundation::{ObjectId, Rounding, Time};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub group: Option<ObjectId>,
    pub state_kind: Kind,
    pub start: Time,
    pub step: Time,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            group: None,
            state_kind: Kind::Content,
            start: Time::ZERO,
            step: Time::new(1, 25).unwrap(),
        }
    }
}
#[derive(Clone)]
pub(crate) struct StepContext {
    pub previous: Value,
    pub delta: Time,
    pub index: i64,
}
fn validate(s: &Settings) -> Result<(), String> {
    if !matches!(s.state_kind, Kind::Content | Kind::Points)
        || s.step <= Time::ZERO
        || s.step < Time::new(1, 1000).unwrap()
    {
        return Err("Solver requires Content or Points state and a step of at least 1 ms".into());
    }
    Ok(())
}
pub fn signature(
    motion: &Motion,
    node: &Node,
) -> Result<(Vec<Socket>, Vec<(String, Kind)>), String> {
    let s = node.settings::<Settings>()?;
    validate(&s)?;
    let mut inputs = vec![Socket::required("initial", s.state_kind)];
    if s.group.is_some() {
        let (ports, outputs) = super::interface::signature(motion, node)?;
        if outputs != vec![("state".into(), s.state_kind)]
            || ports.iter().any(|p| p.id == "initial")
        {
            return Err(
                "Solver update needs one State output of the state type; 'initial' is reserved"
                    .into(),
            );
        }
        inputs.extend(ports);
    }
    Ok((inputs, vec![("content".into(), s.state_kind)]))
}
pub fn definitions() -> Vec<Definition> {
    let mut solver = definition(
        "fold.motion.solver",
        "Solver",
        "Simulation",
        vec![],
        vec![],
        evaluate,
    );
    solver.defaults = || serde_json::to_value(Settings::default()).unwrap();
    solver.validate = |n| validate(&n.settings::<Settings>()?);
    let mut state = definition(
        "fold.motion.solver_state",
        "Previous State",
        "Simulation",
        vec![],
        vec![
            ("state", Kind::Generic),
            ("delta_time", Kind::Scalar),
            ("step", Kind::Scalar),
        ],
        |e, n| {
            let context = e
                .step_context
                .as_ref()
                .ok_or("Previous State is only available inside a Solver update")?;
            let kind: Kind = serde_json::from_value(n.settings["value_type"].clone())
                .map_err(|e| e.to_string())?;
            if context.previous.kind() != kind {
                return Err("Previous State type does not match Solver".into());
            }
            Ok(BTreeMap::from([
                ("state".into(), context.previous.clone()),
                (
                    "delta_time".into(),
                    Value::Field(Field::constant(Datum::Scalar(
                        context.delta.numerator() as f64 / context.delta.denominator() as f64,
                    ))),
                ),
                (
                    "step".into(),
                    Value::Field(Field::constant(Datum::Scalar(context.index as f64))),
                ),
            ]))
        },
    );
    state.defaults = || serde_json::json!({"value_type":"Content"});
    vec![solver, state]
}
fn state(value: Value, kind: Kind) -> Result<Content, String> {
    if value.kind() != kind {
        return Err("Solver update changed state type".into());
    }
    match value {
        Value::Content(c) | Value::Points(c) => Ok(c),
        _ => Err("Solver state must be geometry".into()),
    }
}
fn value(state: Content, kind: Kind) -> Value {
    if kind == Kind::Points {
        Value::Points(state)
    } else {
        Value::Content(state)
    }
}
fn evaluate(e: &mut Evaluator<'_>, node: &Node) -> Result<Outputs, String> {
    if e.step_context.is_some() {
        return Err("nested temporal solvers are not supported".into());
    }
    let settings = node.settings::<Settings>()?;
    validate(&settings)?;
    let group = e
        .motion
        .groups
        .get(&settings.group.ok_or("choose an update graph")?)
        .ok_or("missing update graph")?;
    if e.group_depth >= 32 {
        return Err("solver group nesting limit".into());
    }
    let target = if e.time <= settings.start {
        0
    } else {
        let elapsed = e
            .time
            .checked_sub(settings.start)
            .map_err(|e| e.to_string())?;
        let numerator: u32 = settings
            .step
            .numerator()
            .try_into()
            .map_err(|_| "solver step too large")?;
        elapsed
            .to_ticks(settings.step.denominator(), numerator, Rounding::Floor)
            .map_err(|e| e.to_string())?
    };
    if target > 100_000 {
        return Err("Solver request exceeds 100000 steps; increase the fixed step".into());
    }
    let key = e.simulation_key(node.id)?;
    let (mut index, mut current) = match e.runtime.nearest(&key, target) {
        Some(cached) => cached,
        None => {
            let initial = state(
                e.temporal_input(node, "initial", settings.start)?,
                settings.state_kind,
            )?;
            crate::simulation::state_bytes(&initial, &mut e.budget)?;
            (0, initial)
        }
    };
    while index < target {
        e.budget.spend()?;
        let time = settings
            .start
            .checked_add(
                settings
                    .step
                    .checked_scale(index + 1, 1)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let mut sampled = e.at_time(time)?;
        let result = (|| {
            let mut bindings = BTreeMap::new();
            for port in &group.inputs {
                bindings.insert(port.id.clone(), sampled.input(node, &port.id)?);
            }
            let mut update = Evaluator::for_graph(e.motion, &group.graph, time)?;
            update.bindings = bindings;
            update.group_depth = e.group_depth + 1;
            update.current_object = e.current_object;
            update.object_path = e.object_path.clone();
            update.layer_times = e.layer_times.clone();
            update.step_context = Some(StepContext {
                previous: value(current.clone(), settings.state_kind),
                delta: settings.step,
                index: index + 1,
            });
            update.budget = std::mem::take(&mut sampled.budget);
            let next = update.resolve(group.outputs.get("state").ok_or("missing State output")?);
            sampled.budget = update.budget;
            state(next?, settings.state_kind)
        })();
        e.budget = sampled.budget;
        current = result?;
        let bytes = crate::simulation::state_bytes(&current, &mut e.budget)?;
        index += 1;
        if index % 16 == 0 || index == target {
            e.runtime.insert(&key, index, &current, bytes);
        }
    }
    Ok(output("content", value(current, settings.state_kind)))
}
