//! Reusable image invocation. Hosts supply evaluated images and exact context;
//! providers retain ownership of their editable graph and parameter mappings.
use fold_animation::{Curve, timing::Range};
use fold_foundation::{ObjectId, Time};
use fold_project::{DocumentRef, Metadata};
use fold_render::{ImageId, RenderGraph};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Control {
    pub id: String,
    pub label: String,
    /// Scalar, vector or color components. Component count is part of the interface.
    pub default: Vec<f64>,
    pub range: Option<[f64; 2]>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Interface {
    pub version: u32,
    pub images: Vec<String>,
    pub controls: Vec<Control>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Interface {
    pub fn validate(&self) -> Result<(), String> {
        if self.version == 0 || self.images.len() > 16 || self.controls.len() > 128 {
            return Err("Invalid effect interface version or port budget".into());
        }
        let mut ids = BTreeSet::new();
        for id in self
            .images
            .iter()
            .chain(self.controls.iter().map(|c| &c.id))
        {
            if id.is_empty() || id.len() > 128 || !ids.insert(id) {
                return Err("Effect ports require unique stable identities".into());
            }
        }
        for control in &self.controls {
            if control.label.is_empty()
                || control.label.len() > 128
                || !matches!(control.default.len(), 1..=4)
                || control.default.iter().any(|v| !v.is_finite())
                || control
                    .range
                    .is_some_and(|r| !r.iter().all(|v| v.is_finite()) || r[0] >= r[1])
            {
                return Err("Invalid effect control".into());
            }
        }
        Ok(())
    }
    pub fn values(
        &self,
        overrides: &BTreeMap<String, Vec<f64>>,
    ) -> Result<BTreeMap<String, Vec<f64>>, String> {
        self.validate()?;
        if overrides
            .keys()
            .any(|id| !self.controls.iter().any(|c| &c.id == id))
        {
            return Err(
                "Unavailable effect control; migrate the instance interface explicitly".into(),
            );
        }
        self.controls
            .iter()
            .map(|c| {
                let v = overrides.get(&c.id).unwrap_or(&c.default);
                if v.len() != c.default.len() || v.iter().any(|v| !v.is_finite()) {
                    return Err(format!(
                        "Effect control {} has incompatible components",
                        c.label
                    ));
                }
                Ok((c.id.clone(), v.clone()))
            })
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Instance {
    pub id: ObjectId,
    pub definition: DocumentRef,
    pub version: u32,
    #[serde(default)]
    pub values: BTreeMap<String, Vec<f64>>,
    #[serde(default)]
    pub animation: BTreeMap<String, Vec<Curve>>,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(flatten)]
    pub extensions: Metadata,
}
fn enabled() -> bool {
    true
}
impl Instance {
    /// Relocate host-clock keys with their owning clip, preserving key identity.
    pub fn shift_time(&mut self, delta: Time) -> Result<(), String> {
        let mut animation = self.animation.clone();
        for curve in animation.values_mut().flatten() {
            for key in &mut curve.keys {
                key.time = key.time.checked_add(delta).map_err(|e| e.to_string())?;
            }
        }
        self.animation = animation;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version == 0 || self.values.len() > 128 || self.animation.len() > 128 {
            return Err("Invalid effect instance version/control count".into());
        }
        for value in self.values.values() {
            if !matches!(value.len(), 1..=4) || value.iter().any(|v| !v.is_finite()) {
                return Err("Effect values require finite scalar/vector/color components".into());
            }
        }
        for channels in self.animation.values() {
            if !matches!(channels.len(), 1..=4) {
                return Err("Invalid effect animation components".into());
            }
            for curve in channels {
                curve.validate()?;
            }
        }
        Ok(())
    }
    pub fn values_at(&self, time: Time) -> Result<BTreeMap<String, Vec<f64>>, String> {
        self.validate()?;
        let mut values = self.values.clone();
        for (id, channels) in &self.animation {
            values.insert(
                id.clone(),
                channels
                    .iter()
                    .map(|c| {
                        c.sample(time)
                            .ok_or_else(|| "Empty effect animation".to_string())
                    })
                    .collect::<Result<_, _>>()?,
            );
        }
        Ok(values)
    }
}

/// Image IDs reference a shared immutable prefix. Providers append work to that
/// prefix so repeated input ports do not duplicate upstream processing.
pub struct Invocation<'a> {
    pub graph: &'a RenderGraph,
    pub images: BTreeMap<String, ImageId>,
    pub values: BTreeMap<String, Vec<f64>>,
    pub version: u32,
    pub timing: Range,
    pub format: crate::format::Format,
}
/// Format and timing inherited by ordinary nested graph references. Unlike an
/// invocation this carries no image bindings or promoted parameter overrides.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub timing: Range,
    pub format: crate::format::Format,
}
impl Context {
    /// A child's source clock may be offset from its caller. Preserve progress
    /// while expressing the interval in the clock used to evaluate that child.
    pub fn at_child_time(self, parent: Time, child: Time) -> Result<Self, String> {
        let offset = child.checked_sub(parent).map_err(|e| e.to_string())?;
        Ok(Self {
            timing: Range {
                start: self
                    .timing
                    .start
                    .checked_add(offset)
                    .map_err(|e| e.to_string())?,
                duration: self.timing.duration,
            },
            ..self
        })
    }
}
impl Invocation<'_> {
    pub fn context(&self) -> Context {
        Context {
            timing: self.timing,
            format: self.format,
        }
    }
    pub fn validate(&self, interface: &Interface) -> Result<(), String> {
        interface.validate()?;
        self.timing.validate()?;
        self.format.validate()?;
        if self.version != interface.version {
            return Err("Effect interface changed; explicitly migrate this instance".into());
        }
        if self.images.len() != interface.images.len()
            || interface
                .images
                .iter()
                .any(|id| !self.images.contains_key(id))
            || self.images.values().any(|id| *id >= self.graph.nodes.len())
        {
            return Err("Effect image bindings do not match its interface".into());
        }
        interface.values(&self.values)?;
        Ok(())
    }
}
pub type Resolver<'a> =
    dyn Fn(&DocumentRef, Time, &Invocation<'_>) -> Result<RenderGraph, String> + 'a;

pub fn validate_stack(stack: &[Instance]) -> Result<(), String> {
    if stack.len() > 64 {
        return Err("Effect stack exceeds 64 instances".into());
    }
    let mut ids = BTreeSet::new();
    for instance in stack {
        instance.validate()?;
        if !ids.insert(instance.id) {
            return Err("Duplicate effect instance identity".into());
        }
    }
    Ok(())
}
pub fn apply_stack(
    stack: &[Instance],
    mut image: ImageId,
    graph: &mut RenderGraph,
    time: Time,
    timing: Range,
    format: crate::format::Format,
    resolve: &Resolver<'_>,
) -> Result<ImageId, String> {
    validate_stack(stack)?;
    for instance in stack.iter().filter(|i| i.enabled) {
        let invocation = Invocation {
            graph,
            images: BTreeMap::from([("source".into(), image)]),
            values: instance.values_at(time)?,
            version: instance.version,
            timing,
            format,
        };
        let result = resolve(&instance.definition, time, &invocation)?;
        image = result.output;
        *graph = result;
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_clock_offsets_preserve_instance_progress_and_format() {
        let t = |n, d| Time::new(n, d).unwrap();
        let context = Context {
            timing: Range {
                start: t(100, 1),
                duration: t(3, 2),
            },
            format: crate::format::Format::square([1080, 1920]),
        };
        let child = context.at_child_time(t(201, 2), t(1, 2)).unwrap();
        assert_eq!(child.timing.start, Time::ZERO);
        assert_eq!(
            child
                .timing
                .sample(t(1, 2), fold_animation::timing::Behavior::Clamp)
                .unwrap()
                .progress,
            context
                .timing
                .sample(t(201, 2), fold_animation::timing::Behavior::Clamp)
                .unwrap()
                .progress
        );
        assert_eq!(child.format, context.format);
    }
}

impl Instance {
    /// Adopt a compatible definition version without discarding authored values
    /// or keys. Incompatible identities/components require an explicit edit of
    /// this instance's values/animation before updating its version.
    pub fn adopt_interface(&mut self, interface: &Interface) -> Result<(), String> {
        interface.values(&self.values)?;
        for (id, channels) in &self.animation {
            let control = interface
                .controls
                .iter()
                .find(|c| &c.id == id)
                .ok_or("Remove or rename the unavailable animated control explicitly")?;
            if channels.len() != control.default.len() {
                return Err("Migrate the animated control's component count explicitly".into());
            }
        }
        self.validate()?;
        self.version = interface.version;
        Ok(())
    }
}
