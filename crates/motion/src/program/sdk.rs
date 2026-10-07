// Guest SDK v1. Compiled into an isolated author-time Wasm module, not the host.
// Public helpers return retained native descriptions; no current-frame execution.
use std::collections::BTreeMap;
use std::ops::{Add, Div, Mul, Neg, Sub};

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn object(values: &BTreeMap<String, String>) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|(k, v)| format!("{}:{}", quote(k), v))
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub fn key(prefix: &str, index: usize) -> String {
    format!("{prefix}_{index}")
}

/// Symbolic native value. Clone when reusing it; arithmetic retains dependencies.
#[derive(Clone)]
pub struct Signal {
    expression: String,
}
impl Signal {
    fn raw(expression: String) -> Self {
        Self { expression }
    }
    fn literal(datum: String) -> Self {
        Self::raw(format!("{{\"op\":\"literal\",\"value\":{datum}}}"))
    }
    fn unary(self, operation: &str) -> Self {
        Self::raw(format!(
            "{{\"op\":\"unary\",\"operation\":{},\"value\":{}}}",
            quote(operation),
            self.expression
        ))
    }
    fn binary(self, operation: &str, b: impl Into<Signal>) -> Self {
        Self::raw(format!(
            "{{\"op\":\"binary\",\"operation\":{},\"a\":{},\"b\":{}}}",
            quote(operation),
            self.expression,
            b.into().expression
        ))
    }
    pub fn sin(self) -> Self {
        self.unary("sin")
    }
    pub fn cos(self) -> Self {
        self.unary("cos")
    }
    pub fn floor(self) -> Self {
        self.unary("floor")
    }
    pub fn abs(self) -> Self {
        self.unary("abs")
    }
    pub fn modulo(self, other: impl Into<Signal>) -> Self {
        self.binary("modulo", other)
    }
    pub fn min(self, other: impl Into<Signal>) -> Self {
        self.binary("min", other)
    }
    /// Clamp a scalar signal. Unlike a control's drag range, this affects evaluation.
    pub fn clamp(self, low: impl Into<Signal>, high: impl Into<Signal>) -> Self {
        self.max(low).min(high)
    }
    /// Cubic ease-in/out of a scalar progress value, clamped to 0..1.
    pub fn smooth(self) -> Self {
        let t = self.clamp(0., 1.);
        t.clone() * t.clone() * (Signal::from(3.) - t * 2.)
    }
    /// Scalar interpolation; progress is not implicitly clamped.
    pub fn mix(self, to: impl Into<Signal>, progress: impl Into<Signal>) -> Self {
        self.clone() + (to.into() - self) * progress.into()
    }
    pub fn max(self, other: impl Into<Signal>) -> Self {
        self.binary("max", other)
    }
}
impl From<f64> for Signal {
    fn from(v: f64) -> Self {
        assert!(v.is_finite());
        Self::literal(format!("{{\"Scalar\":{v}}}"))
    }
}
impl From<[f64; 2]> for Signal {
    fn from(v: [f64; 2]) -> Self {
        vec2(v[0], v[1])
    }
}
impl From<[f64; 4]> for Signal {
    fn from(v: [f64; 4]) -> Self {
        assert!(v.iter().all(|x| x.is_finite()));
        Self::literal(format!("{{\"Color\":{:?}}}", v))
    }
}
impl From<&str> for Signal {
    fn from(v: &str) -> Self {
        Self::literal(format!("{{\"Text\":{}}}", quote(v)))
    }
}
impl From<bool> for Signal {
    fn from(v: bool) -> Self {
        Self::literal(format!("{{\"Bool\":{v}}}"))
    }
}
impl<T: Into<Signal>> Add<T> for Signal {
    type Output = Self;
    fn add(self, b: T) -> Self {
        self.binary("add", b)
    }
}
impl<T: Into<Signal>> Sub<T> for Signal {
    type Output = Self;
    fn sub(self, b: T) -> Self {
        self.binary("subtract", b)
    }
}
impl<T: Into<Signal>> Mul<T> for Signal {
    type Output = Self;
    fn mul(self, b: T) -> Self {
        self.binary("multiply", b)
    }
}
impl<T: Into<Signal>> Div<T> for Signal {
    type Output = Self;
    fn div(self, b: T) -> Self {
        self.binary("divide", b)
    }
}
impl Neg for Signal {
    type Output = Self;
    fn neg(self) -> Self {
        self.unary("negate")
    }
}
pub fn time() -> Signal {
    Signal::raw("{\"op\":\"time\"}".into())
}
pub fn index() -> Signal {
    Signal::raw("{\"op\":\"index\"}".into())
}
pub fn count() -> Signal {
    Signal::raw("{\"op\":\"count\"}".into())
}
pub fn vec2(x: impl Into<Signal>, y: impl Into<Signal>) -> Signal {
    Signal::raw(format!(
        "{{\"op\":\"vector\",\"x\":{},\"y\":{}}}",
        x.into().expression,
        y.into().expression
    ))
}

#[derive(Clone)]
pub struct Output {
    key: String,
    socket: String,
}
impl Output {
    pub fn socket(&self, socket: &str) -> Self {
        Self {
            key: self.key.clone(),
            socket: socket.into(),
        }
    }
    fn json(&self) -> String {
        format!(
            "{{\"key\":{},\"socket\":{}}}",
            quote(&self.key),
            quote(&self.socket)
        )
    }
}

/// Low-level escape hatch. Settings are validated by the native node provider.
pub struct Node {
    key: String,
    kind: String,
    settings: String,
    inputs: BTreeMap<String, String>,
}
impl Node {
    pub fn new(key: &str, kind: &str) -> Self {
        Self {
            key: key.into(),
            kind: kind.into(),
            settings: "{}".into(),
            inputs: BTreeMap::new(),
        }
    }
    pub fn value(mut self, input: &str, value: impl Into<Signal>) -> Self {
        self.inputs.insert(
            input.into(),
            format!("{{\"expression\":{}}}", value.into().expression),
        );
        self
    }
    pub fn link(mut self, input: &str, value: &Output) -> Self {
        self.inputs
            .insert(input.into(), format!("{{\"link\":{}}}", value.json()));
        self
    }
    pub fn settings(mut self, json: &str) -> Self {
        self.settings = json.into();
        self
    }
    fn json(&self) -> String {
        format!(
            "{{\"key\":{},\"kind\":{},\"settings\":{},\"inputs\":{}}}",
            quote(&self.key),
            quote(&self.kind),
            self.settings,
            object(&self.inputs)
        )
    }
}

/// Retained time helpers. Durations are protected against zero/negative controls.
pub fn progress(start: impl Into<Signal>, duration: impl Into<Signal>) -> Signal {
    ((time() - start.into()) / duration.into().max(0.000001)).clamp(0., 1.)
}
pub fn phase(period: impl Into<Signal>) -> Signal {
    (time() / period.into().max(0.000001)).modulo(1.)
}
pub fn stagger(
    start: impl Into<Signal>,
    duration: impl Into<Signal>,
    delay: impl Into<Signal>,
) -> Signal {
    progress(start.into() + index() * delay.into(), duration)
}
struct Control {
    id: String,
    label: String,
    kind: String,
    datum: String,
    unit: String,
    section: String,
    advanced: bool,
    range: Option<[f64; 2]>,
    description: String,
}
impl Control {
    fn json(&self) -> String {
        let range = self
            .range
            .map(|r| format!("{:?}", r))
            .unwrap_or_else(|| "null".into());
        format!(
            "{{\"id\":{},\"name\":{},\"kind\":{},\"default\":{},\"unit\":{},\"section\":{},\"advanced\":{},\"range\":{},\"description\":{}}}",
            quote(&self.id),
            quote(&self.label),
            quote(&self.kind),
            self.datum,
            quote(&self.unit),
            quote(&self.section),
            self.advanced,
            range,
            quote(&self.description)
        )
    }
}
pub struct Asset {
    name: String,
    ports: Vec<Control>,
    nodes: Vec<Node>,
    output: Option<Output>,
}
impl Asset {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ports: vec![],
            nodes: vec![],
            output: None,
        }
    }
    fn control(&mut self, id: &str, label: &str, kind: &str, datum: String, unit: &str) -> Signal {
        self.ports.push(Control {
            id: id.into(),
            label: label.into(),
            kind: kind.into(),
            datum,
            unit: unit.into(),
            section: "Controls".into(),
            advanced: false,
            range: None,
            description: String::new(),
        });
        Signal::raw(format!("{{\"op\":\"input\",\"name\":{}}}", quote(id)))
    }
    pub fn scalar(&mut self, id: &str, label: &str, value: f64, unit: &str) -> Signal {
        assert!(value.is_finite());
        self.control(id, label, "Scalar", format!("{{\"Scalar\":{value}}}"), unit)
    }
    pub fn scalar_range(
        &mut self,
        id: &str,
        label: &str,
        value: f64,
        unit: &str,
        range: [f64; 2],
    ) -> Signal {
        assert!(range.iter().all(|v| v.is_finite()) && range[0] < range[1]);
        let signal = self.scalar(id, label, value, unit);
        self.ports.last_mut().unwrap().range = Some(range);
        signal
    }
    /// Presentation only: preserves the control's stable ID and evaluation.
    pub fn describe(&mut self, id: &str, section: &str, advanced: bool, description: &str) {
        let port = self
            .ports
            .iter_mut()
            .find(|p| p.id == id)
            .expect("declare the control first");
        port.section = section.into();
        port.advanced = advanced;
        port.description = description.into();
    }
    pub fn text_control(&mut self, id: &str, label: &str, value: &str) -> Signal {
        self.control(
            id,
            label,
            "Text",
            format!("{{\"Text\":{}}}", quote(value)),
            "",
        )
    }
    /// Colors are authored in the project's working color interpretation.
    pub fn color(&mut self, id: &str, label: &str, value: [f64; 4]) -> Signal {
        assert!(value.iter().all(|x| x.is_finite()));
        self.control(id, label, "Color", format!("{{\"Color\":{:?}}}", value), "")
    }
    pub fn vector(&mut self, id: &str, label: &str, value: [f64; 2], unit: &str) -> Signal {
        assert!(value.iter().all(|x| x.is_finite()));
        self.control(
            id,
            label,
            "Vector",
            format!("{{\"Vector\":{:?}}}", value),
            unit,
        )
    }
    pub fn input(&mut self, id: &str, label: &str, kind: &str) -> Output {
        self.control(id, label, kind, "null".into(), "");
        self.ports.last_mut().unwrap().section = "Sources".into();
        Output {
            key: format!("@{id}"),
            socket: "value".into(),
        }
    }
    pub fn add(&mut self, node: Node) -> Output {
        assert!(self.nodes.len() < 1024);
        let out = Output {
            key: node.key.clone(),
            socket: "content".into(),
        };
        self.nodes.push(node);
        out
    }
    /// Native scalar keyframes at exact rational seconds: (numerator, denominator, value).
    /// Connect the returned value output with Node::link.
    pub fn keyframes(
        &mut self,
        key: &str,
        keys: &[(i64, u32, f64)],
        interpolation: &str,
    ) -> Output {
        assert!(!keys.is_empty() && keys.len() <= 256);
        assert!(matches!(interpolation, "Hold" | "Linear" | "Ease"));
        let values=keys.iter().map(|(n,d,v)| {
            assert!(*d>0 && v.is_finite());
            format!("{{\"time\":{{\"numerator\":{n},\"denominator\":{d}}},\"value\":{{\"Scalar\":{v}}},\"interpolation\":{}}}",quote(interpolation))
        }).collect::<Vec<_>>().join(",");
        self.add(
            Node::new(key, "fold.motion.keyframes")
                .settings(&format!("{{\"keys\":[{values}],\"channels\":[]}}")),
        )
        .socket("value")
    }
    pub fn text(&mut self, key: &str, text: impl Into<Signal>, size: impl Into<Signal>) -> Output {
        self.add(
            Node::new(key, "fold.motion.text")
                .value("text", text)
                .value("size", size),
        )
    }
    pub fn circle(&mut self, key: &str, radius: impl Into<Signal>) -> Output {
        let d = radius.into() * 2.;
        let shape = self.add(
            Node::new(&format!("{key}/shape"), "fold.motion.ellipse")
                .value("width", d.clone())
                .value("height", d),
        );
        self.align(key, &shape, [0., 0.])
    }
    pub fn rectangle(
        &mut self,
        key: &str,
        width: impl Into<Signal>,
        height: impl Into<Signal>,
    ) -> Output {
        self.add(
            Node::new(key, "fold.motion.rectangle")
                .value("width", width)
                .value("height", height),
        )
    }
    pub fn fill(&mut self, key: &str, source: &Output, color: impl Into<Signal>) -> Output {
        self.add(
            Node::new(key, "fold.motion.fill")
                .link("content", source)
                .value("color", color),
        )
    }
    pub fn align(&mut self, key: &str, source: &Output, target: impl Into<Signal>) -> Output {
        self.add(
            Node::new(key, "fold.motion.align_content")
                .link("content", source)
                .value("target", target),
        )
    }
    /// Uniformly fit measured outlines inside a box, following changing text and size.
    pub fn fit(
        &mut self,
        key: &str,
        source: &Output,
        origin: impl Into<Signal>,
        size: impl Into<Signal>,
    ) -> Output {
        self.add(
            Node::new(key, "fold.motion.fit_content")
                .link("content", source)
                .value("origin", origin)
                .value("size", size),
        )
    }
    pub fn align_anchor(
        &mut self,
        key: &str,
        source: &Output,
        target: impl Into<Signal>,
        anchor: impl Into<Signal>,
    ) -> Output {
        self.add(
            Node::new(key, "fold.motion.align_content")
                .link("content", source)
                .value("target", target)
                .value("anchor", anchor),
        )
    }
    pub fn translate(&mut self, key: &str, source: &Output, offset: impl Into<Signal>) -> Output {
        self.add(
            Node::new(key, "fold.motion.transform")
                .link("content", source)
                .value("translation", offset),
        )
    }
    /// Add an offset to each retained instance, sampling index/count per copy.
    pub fn translate_each(
        &mut self,
        key: &str,
        source: &Output,
        offset: impl Into<Signal>,
    ) -> Output {
        self.add(
            Node::new(key, "fold.motion.set_position")
                .link("content", source)
                .value("value", offset),
        )
    }
    pub fn grid(
        &mut self,
        key: &str,
        columns: impl Into<Signal>,
        rows: impl Into<Signal>,
        spacing: impl Into<Signal>,
        origin: impl Into<Signal>,
    ) -> Output {
        self.add(
            Node::new(key, "fold.motion.grid_points")
                .value("columns", columns)
                .value("rows", rows)
                .value("spacing", spacing)
                .value("origin", origin),
        )
        .socket("points")
    }
    pub fn shape_points(
        &mut self,
        key: &str,
        source: &Output,
        spacing: impl Into<Signal>,
    ) -> Output {
        self.add(
            Node::new(key, "fold.motion.shape_points")
                .link("content", source)
                .value("spacing", spacing),
        )
        .socket("points")
    }
    pub fn copies(&mut self, key: &str, source: &Output, points: &Output) -> Output {
        self.add(
            Node::new(key, "fold.motion.instance_on_points")
                .link("source", source)
                .link("points", points),
        )
    }
    pub fn merge(&mut self, key: &str, back: &Output, front: &Output) -> Output {
        self.add(
            Node::new(key, "fold.motion.group")
                .link("back", back)
                .link("front", front),
        )
    }
    /// Custom element code uses the existing bounded runtime Rust Wrangle.
    pub fn wrangle(
        &mut self,
        key: &str,
        source: &Output,
        domain: &str,
        code: &str,
        channels: &[(&str, Signal)],
    ) -> Output {
        let kind = if domain == "Points" {
            "Points"
        } else {
            "Content"
        };
        let settings = format!(
            "{{\"source\":{},\"abi\":1,\"domain\":{},\"geometry_kind\":{},\"channels\":[]}}",
            quote(code),
            quote(domain),
            quote(kind)
        );
        let mut n = Node::new(key, "fold.motion.wrangle")
            .link("content", source)
            .settings(&settings);
        for (name, value) in channels {
            n = n.value(name, value.clone());
        }
        self.add(n)
    }
    pub fn output(&mut self, source: &Output) {
        self.output = Some(source.clone());
    }
    fn finish(self) -> String {
        format!(
            "{{\"version\":1,\"name\":{},\"inputs\":[{}],\"nodes\":[{}],\"output\":{}}}",
            quote(&self.name),
            self.ports
                .iter()
                .map(Control::json)
                .collect::<Vec<_>>()
                .join(","),
            self.nodes
                .iter()
                .map(Node::json)
                .collect::<Vec<_>>()
                .join(","),
            self.output.expect("asset.output is required").json()
        )
    }
}
