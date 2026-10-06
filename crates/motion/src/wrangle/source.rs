//! Parse real Rust for channel discovery and reject compile-time host access.
use crate::fields::{Datum, Kind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use syn::visit::{self, Visit};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub kind: Kind,
    pub default: Datum,
}
#[derive(Default)]
struct Discovery {
    channels: BTreeMap<String, Kind>,
    attributes: BTreeMap<String, Kind>,
    errors: Vec<String>,
}
impl<'a> Visit<'a> for Discovery {
    fn visit_attribute(&mut self, _: &'a syn::Attribute) {
        self.errors
            .push("Rust attributes are unavailable in wrangles".into());
    }
    fn visit_macro(&mut self, _: &'a syn::Macro) {
        self.errors
            .push("Rust macros are unavailable in wrangles (including include!/env!)".into());
    }
    fn visit_item(&mut self, i: &'a syn::Item) {
        if matches!(i, syn::Item::Fn(_)) {
            visit::visit_item(self, i);
        } else {
            self.errors.push("Only local functions and expressions are supported; modules, imports and static state are unavailable".into());
        }
    }
    fn visit_expr_call(&mut self, c: &'a syn::ExprCall) {
        if let syn::Expr::Path(p) = c.func.as_ref() {
            if let Some(name) = p.path.get_ident() {
                let name = name.to_string();
                let attribute = name.starts_with("attr") || name.starts_with("set_attr");
                let setter = name.starts_with("set_attr");
                let kind = match name.as_str() {
                    "chf" | "attrf" | "set_attrf" => Some(Kind::Scalar),
                    "chv" | "attrv" | "set_attrv" => Some(Kind::Vector),
                    "chc" | "attrc" | "set_attrc" => Some(Kind::Color),
                    "chb" | "attrb" | "set_attrb" => Some(Kind::Bool),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let key = match c.args.first() {
                        Some(syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        })) if c.args.len() == if setter { 2 } else { 1 } => Some(s.value()),
                        _ => None,
                    };
                    if let Some(key) =
                        key.filter(|k| !k.is_empty() && k.len() <= 80 && k != "content")
                    {
                        if (if attribute {
                            &mut self.attributes
                        } else {
                            &mut self.channels
                        })
                        .insert(key.clone(), kind)
                        .is_some_and(|old| old != kind)
                        {
                            self.errors
                                .push(format!("channel {key} is used with incompatible types"));
                        }
                    } else {
                        self.errors.push("channel calls require one literal name (1..80 bytes, excluding content)".into());
                    }
                }
            }
        }
        visit::visit_expr_call(self, c);
    }
}
pub fn discover(source: &str) -> Result<Vec<Channel>, String> {
    Ok(bindings(source)?.0)
}
pub fn attributes(source: &str) -> Result<Vec<Channel>, String> {
    Ok(bindings(source)?.1)
}
pub(super) fn bindings(source: &str) -> Result<(Vec<Channel>, Vec<Channel>), String> {
    if source.len() > 32768 {
        return Err("wrangle source exceeds 32 KiB".into());
    }
    // Conservative lexical bounds precede recursive Rust parsing/AST traversal.
    // Count comments and strings too: false rejections are preferable to a host
    // stack overflow while parsing an adversarial deeply nested draft.
    if source.bytes().filter(|b| b.is_ascii_punctuation()).count() > 1024
        || source
            .bytes()
            .filter(|b| matches!(b, b'(' | b'[' | b'{'))
            .count()
            > 128
        || source.split_whitespace().count() > 512
    {
        return Err("wrangle exceeds structural parsing budget (128 open delimiters, 1024 punctuation characters, 512 words)".into());
    }
    let block: syn::Block =
        syn::parse_str(&format!("{{\n{source}\n}}")).map_err(|e| format!("Rust syntax: {e}"))?;
    let mut d = Discovery::default();
    d.visit_block(&block);
    if !d.errors.is_empty() {
        return Err(d.errors.join("\n"));
    }
    if d.channels.len() > 64 || d.attributes.len() > 64 {
        return Err("wrangle supports at most 64 channels".into());
    }
    let convert = |values: BTreeMap<String, Kind>| {
        values
            .into_iter()
            .map(|(id, kind)| Channel {
                id,
                kind,
                default: match kind {
                    Kind::Vector => Datum::Vector([0.; 2]),
                    Kind::Color => Datum::Color([1.; 4]),
                    Kind::Bool => Datum::Bool(false),
                    _ => Datum::Scalar(0.),
                },
            })
            .collect()
    };
    Ok((convert(d.channels), convert(d.attributes)))
}
pub fn module(source: &str, channels: &[Channel], attributes: &[Channel]) -> String {
    let mut wrappers = String::new();
    for (prefix, bindings, base) in [
        ("ch", channels, 32),
        ("attr", attributes, 32 + channels.len() * 4),
    ] {
        for (suffix, kind, ret, width) in [
            ("f", Kind::Scalar, "f64", 1),
            ("v", Kind::Vector, "[f64;2]", 2),
            ("c", Kind::Color, "[f64;4]", 4),
            ("b", Kind::Bool, "bool", 1),
        ] {
            let name = format!("{prefix}{suffix}");
            wrappers.push_str(&format!("fn {name}(name:&str)->{ret} {{ match name {{"));
            for (i, c) in bindings.iter().enumerate().filter(|(_, c)| c.kind == kind) {
                let slot = base + i * 4;
                let val = if kind == Kind::Bool {
                    format!("read({slot}) != 0.0")
                } else if width == 1 {
                    format!("read({slot})")
                } else {
                    format!(
                        "[{}]",
                        (0..width)
                            .map(|j| format!("read({})", slot + j))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                };
                wrappers.push_str(&format!("{:?} => {val},", c.id));
            }
            wrappers.push_str("_ => core::arch::wasm32::unreachable(), }}\n");
        }
        if prefix == "attr" {
            for (suffix, kind, ret, width) in [
                ("f", Kind::Scalar, "f64", 1),
                ("v", Kind::Vector, "[f64;2]", 2),
                ("c", Kind::Color, "[f64;4]", 4),
                ("b", Kind::Bool, "bool", 1),
            ] {
                wrappers.push_str(&format!(
                    "fn set_attr{suffix}(name:&str,value:{ret}) {{match name {{"
                ));
                for (i, c) in attributes
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.kind == kind)
                {
                    wrappers.push_str(&format!("{:?} => {{", c.id));
                    for j in 0..width {
                        let v = if kind == Kind::Bool {
                            "if value {1.0}else{0.0}".into()
                        } else if width == 1 {
                            "value".into()
                        } else {
                            format!("value[{j}]")
                        };
                        wrappers.push_str(&format!("write({},{});", base + i * 4 + j, v));
                    }
                    wrappers.push_str(&format!(
                        "write({},1.0);}},",
                        base + attributes.len() * 4 + i
                    ));
                }
                wrappers.push_str("_=>core::arch::wasm32::unreachable(),}}\n");
            }
        }
    }
    format!(
        r#"// Fixed-size numeric exchange; guest code cannot dereference host pointers.
static mut VALUES: [f64; 608] = [0.; 608];
static mut EXACT: [i64; 5] = [0; 5];
#[unsafe(no_mangle)]
pub extern "C" fn values_ptr() -> *mut f64 {{ core::ptr::addr_of_mut!(VALUES).cast() }}
#[unsafe(no_mangle)]
pub extern "C" fn exact_ptr() -> *mut i64 {{ core::ptr::addr_of_mut!(EXACT).cast() }}
fn read(slot:i32)->f64 {{unsafe{{ if (0..608).contains(&slot) {{ VALUES[slot as usize] }} else {{ f64::NAN }} }}}}
fn write(slot:i32,value:f64) {{unsafe{{ if (0..608).contains(&slot) {{ VALUES[slot as usize] = value; }} }}}}
fn exact(slot:i32)->i64 {{unsafe{{ if (0..5).contains(&slot) {{ EXACT[slot as usize] }} else {{ 0 }} }}}}
{wrappers}
#[derive(Clone,Copy)] struct Time {{ numerator:i64, denominator:u32, seconds:f64 }}
#[unsafe(no_mangle)]
pub extern "C" fn run() {{
    let mut position=[read(0),read(1)];
    let mut color=[read(2),read(3),read(4),read(5)];
    let mut opacity=read(6);
    let time=Time{{numerator:exact(0),denominator:exact(1) as u32,seconds:read(7)}};
    let index=exact(2) as usize; let count=exact(3) as usize; let id=exact(4) as u64;
    let delta_time=read(8);
    let tau=core::f64::consts::TAU;
    #[deny(unsafe_code)]
    {{
// FOLD_USER_SOURCE
{source}
    }}
    write(0,position[0]);write(1,position[1]);
    write(2,color[0]);write(3,color[1]);write(4,color[2]);write(5,color[3]);write(6,opacity);
}}
"#
    )
}

/// Conservative proof for reusing one guest across elements. Only local numeric
/// operations and our generated accessors are allowed; arbitrary Rust retains
/// fresh-instance execution. In particular there is no allocator, guest memory
/// intrinsic, closure, user function, macro, external path or indirect call.
pub(super) fn element_local(source: &str) -> bool {
    struct Local(bool);
    impl<'a> Visit<'a> for Local {
        fn visit_item(&mut self, _: &'a syn::Item) {
            self.0 = false;
        }
        fn visit_path(&mut self, p: &'a syn::Path) {
            if p.leading_colon.is_some() || p.segments.len() != 1 {
                self.0 = false;
            }
            visit::visit_path(self, p);
        }
        fn visit_expr(&mut self, e: &'a syn::Expr) {
            use syn::Expr::*;
            if !matches!(
                e,
                Array(_)
                    | Assign(_)
                    | Binary(_)
                    | Block(_)
                    | Break(_)
                    | Cast(_)
                    | Continue(_)
                    | Field(_)
                    | ForLoop(_)
                    | Group(_)
                    | If(_)
                    | Index(_)
                    | Let(_)
                    | Lit(_)
                    | Loop(_)
                    | Paren(_)
                    | Path(_)
                    | Range(_)
                    | Reference(_)
                    | Return(_)
                    | Tuple(_)
                    | Unary(_)
                    | While(_)
                    | Call(_)
                    | MethodCall(_)
            ) {
                self.0 = false;
            }
            visit::visit_expr(self, e);
        }
        fn visit_expr_call(&mut self, c: &'a syn::ExprCall) {
            let allowed = match c.func.as_ref() {
                syn::Expr::Path(p) => p.path.get_ident().is_some_and(|name| {
                    matches!(
                        name.to_string().as_str(),
                        "chf"
                            | "chv"
                            | "chc"
                            | "chb"
                            | "attrf"
                            | "attrv"
                            | "attrc"
                            | "attrb"
                            | "set_attrf"
                            | "set_attrv"
                            | "set_attrc"
                            | "set_attrb"
                    )
                }),
                _ => false,
            };
            self.0 &= allowed;
            visit::visit_expr_call(self, c);
        }
        fn visit_expr_method_call(&mut self, c: &'a syn::ExprMethodCall) {
            self.0 &= matches!(
                c.method.to_string().as_str(),
                "abs"
                    | "min"
                    | "max"
                    | "clamp"
                    | "floor"
                    | "ceil"
                    | "round"
                    | "trunc"
                    | "fract"
                    | "sin"
                    | "cos"
                    | "tan"
                    | "asin"
                    | "acos"
                    | "atan"
                    | "atan2"
                    | "sqrt"
                    | "cbrt"
                    | "powf"
                    | "powi"
                    | "exp"
                    | "exp2"
                    | "ln"
                    | "log2"
                    | "log10"
                    | "hypot"
                    | "signum"
                    | "rem_euclid"
                    | "div_euclid"
                    | "mul_add"
                    | "to_radians"
                    | "to_degrees"
                    | "is_finite"
                    | "is_nan"
                    | "is_infinite"
            );
            visit::visit_expr_method_call(self, c);
        }
    }
    let Ok(block) = syn::parse_str::<syn::Block>(&format!("{{{source}}}")) else {
        return false;
    };
    let mut local = Local(true);
    local.visit_block(&block);
    local.0
}

#[cfg(test)]
mod locality_tests {
    use super::element_local;
    #[test]
    fn reuse_requires_only_local_numeric_operations() {
        assert!(element_local(
            "let x = chf(\"x\"); position[1] = (x + time.seconds).sin(); set_attrv(\"scale\", [x, 1.]);"
        ));
        for source in [
            "let x = Vec::<f64>::with_capacity(10);",
            "core::arch::wasm32::memory_grow::<0>(1);",
            "let x = \"x\".repeat(10);",
            "fn f() {} f();",
            "let f = || 1.; position[0] = f();",
            "let f = run; f();",
            "position[0] = read(1);",
            "let x = std::f64::consts::PI;",
        ] {
            assert!(!element_local(source), "{source}");
        }
    }
}
