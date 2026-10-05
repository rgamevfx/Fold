use super::*;
use crate::{
    evaluation::content,
    fields::{Budget, Field, Sample},
    geometry::{Content, Domain, Element, Geometry, multiply, transform},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub enum Combine {
    Replace,
    #[default]
    Add,
    Multiply,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub domain: Domain,
    pub combine: Combine,
}
fn settings() -> serde_json::Value {
    serde_json::to_value(Settings {
        domain: Domain::Instances,
        combine: Combine::Add,
    })
    .unwrap()
}
type Response = fn(&mut Element, Datum, f64, Combine) -> Result<(), String>;
fn definition_response(
    id: &'static str,
    name: &'static str,
    value: Socket,
    evaluate: fn(&mut Evaluator<'_>, &Node) -> Result<Outputs, String>,
) -> Definition {
    let mut def = definition(
        id,
        name,
        "Response",
        vec![
            Socket::required("content", Kind::Content),
            value,
            boolean("selection", true),
            scalar("weight", 1., "factor"),
        ],
        vec![("content", Kind::Content)],
        evaluate,
    );
    def.defaults = settings;
    def.signature = Some(|n| {
        let settings = n.settings::<Settings>()?;
        let def = crate::graph::registry::find(&n.kind)?;
        let mut inputs = def.inputs.clone();
        let kind = if settings.domain == Domain::Points {
            Kind::Points
        } else {
            Kind::Content
        };
        inputs[0].kind = kind;
        Ok((inputs, vec![("content".into(), kind)]))
    });
    def.validate = |n| n.settings::<Settings>().map(|_| ());
    def
}
pub fn definitions() -> Vec<Definition> {
    let mut definitions = vec![
        definition_response(
            "fold.motion.set_position",
            "Set Position",
            vector("value", [0., 0.], "px"),
            |e, n| apply(e, n, position),
        ),
        definition_response(
            "fold.motion.set_rotation",
            "Set Rotation",
            scalar("value", 0., "degrees"),
            |e, n| apply(e, n, rotation),
        ),
        definition_response(
            "fold.motion.set_scale",
            "Set Scale",
            vector("value", [1., 1.], "factor"),
            |e, n| apply(e, n, scale),
        ),
        definition_response(
            "fold.motion.set_color_opacity",
            "Set Color / Opacity",
            color("value", [1., 1., 1., 1.]),
            |e, n| apply(e, n, color_response),
        ),
    ];
    definitions[2].defaults = || {
        serde_json::to_value(Settings {
            domain: Domain::Instances,
            combine: Combine::Multiply,
        })
        .unwrap()
    };
    definitions[3].defaults = || {
        serde_json::to_value(Settings {
            domain: Domain::Instances,
            combine: Combine::Replace,
        })
        .unwrap()
    };
    definitions
}
fn position(e: &mut Element, value: Datum, w: f64, mode: Combine) -> Result<(), String> {
    let v = value.vector()?;
    for (i, v) in v.into_iter().enumerate() {
        let base = e.transform[4 + i];
        e.transform[4 + i] = match mode {
            Combine::Replace => base + (v - base) * w,
            Combine::Add => base + v * w,
            Combine::Multiply => return Err("position supports Replace or Add".into()),
        };
    }
    Ok(())
}
fn rotation(e: &mut Element, value: Datum, w: f64, mode: Combine) -> Result<(), String> {
    let value = value.scalar()?;
    let angle = match mode {
        Combine::Add => value * w,
        Combine::Replace => (value - e.transform[1].atan2(e.transform[0]).to_degrees()) * w,
        Combine::Multiply => return Err("rotation supports Replace or Add".into()),
    };
    let [x, y] = [e.transform[4], e.transform[5]];
    e.transform = multiply(transform([0., 0.], angle, [1., 1.], [0., 0.]), e.transform);
    e.transform[4] = x;
    e.transform[5] = y;
    Ok(())
}
fn scale(e: &mut Element, value: Datum, w: f64, mode: Combine) -> Result<(), String> {
    let v = value.vector()?;
    let current = [
        e.transform[0].hypot(e.transform[1]),
        e.transform[2].hypot(e.transform[3]),
    ];
    let factors = match mode {
        Combine::Multiply => [1. + (v[0] - 1.) * w, 1. + (v[1] - 1.) * w],
        Combine::Replace => {
            if current.contains(&0.) {
                return Err("cannot replace scale on singular transform".into());
            }
            [
                1. + (v[0] / current[0] - 1.) * w,
                1. + (v[1] / current[1] - 1.) * w,
            ]
        }
        Combine::Add => return Err("scale supports Replace or Multiply".into()),
    };
    e.transform = multiply(e.transform, transform([0., 0.], 0., factors, [0., 0.]));
    Ok(())
}
fn color_response(e: &mut Element, value: Datum, w: f64, mode: Combine) -> Result<(), String> {
    let v = value.color()?;
    let base = e.fill.unwrap_or([0., 0., 0., 1.]);
    let color = std::array::from_fn(|i| match mode {
        Combine::Replace => base[i] + (v[i] - base[i]) * w,
        Combine::Add => base[i] + v[i] * w,
        Combine::Multiply => base[i] * (1. + (v[i] - 1.) * w),
    });
    if color.iter().any(|v| !v.is_finite()) || !(0. ..=1.).contains(&color[3]) {
        return Err("response requires finite color and alpha in 0..1".into());
    }
    e.fill = Some(color);
    Ok(())
}
fn apply(e: &mut Evaluator<'_>, n: &Node, response: Response) -> Result<Outputs, String> {
    let source = e.content(n, "content")?;
    let settings = n.settings::<Settings>()?;
    let value = e.field(n, "value")?;
    let selection = e.field(n, "selection")?;
    let weight = e.field(n, "weight")?;
    let count = count(&source, settings.domain, 0)?;
    if count == 0 && !source.is_empty() {
        return Err(format!("no elements in {:?} domain", settings.domain));
    }
    let mut state = Apply {
        domain: settings.domain,
        mode: settings.combine,
        value,
        selection,
        weight,
        response,
        index: 0,
        count,
    };
    let result = state.walk(&source, &mut e.budget, 0)?;
    if settings.domain == Domain::Points {
        Ok(crate::evaluation::output(
            "content",
            crate::evaluation::Value::Points(result),
        ))
    } else {
        Ok(content(result))
    }
}
fn count(content: &Content, domain: Domain, depth: usize) -> Result<usize, String> {
    if depth > 64 {
        return Err("response nesting exceeds 64".into());
    }
    let mut total = 0;
    for item in content.iter() {
        if item.matches(domain) {
            total += 1;
        } else if let Geometry::Group(children) = &item.geometry {
            total += count(children, domain, depth + 1)?;
        }
    }
    if total > 16384 {
        return Err("response domain exceeds 16384 elements".into());
    }
    Ok(total)
}
struct Apply {
    domain: Domain,
    mode: Combine,
    value: Field,
    selection: Field,
    weight: Field,
    response: Response,
    index: usize,
    count: usize,
}
impl Apply {
    fn walk(&mut self, source: &Content, b: &mut Budget, depth: usize) -> Result<Content, String> {
        if depth > 64 {
            return Err("response nesting exceeds 64".into());
        }
        let mut result = Vec::with_capacity(source.len());
        for item in source.iter() {
            b.spend()?;
            let mut item = item.clone();
            if item.matches(self.domain) {
                let sample = Sample {
                    time_offset: None,
                    position: [item.transform[4], item.transform[5]],
                    index: self.index,
                    id: item.id,
                    count: self.count,
                };
                self.index += 1;
                if self.selection.sample(&sample, b)?.boolean()? {
                    let w = self.weight.sample(&sample, b)?.scalar()?;
                    if !(0. ..=1.).contains(&w) {
                        return Err("response weight outside 0..=1".into());
                    }
                    if w != 0. {
                        (self.response)(&mut item, self.value.sample(&sample, b)?, w, self.mode)?;
                    }
                }
            } else if let Geometry::Group(children) = &item.geometry {
                item.geometry = Geometry::Group(self.walk(children, b, depth + 1)?);
            }
            result.push(item);
        }
        Ok(Arc::new(result))
    }
}
