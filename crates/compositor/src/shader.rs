//! Stable authoring interface for a GPU image function.
use fold_platform::effects::Control;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shader {
    pub source: String,
    pub images: Vec<String>,
    pub controls: Vec<Control>,
}
impl Default for Shader {
    fn default() -> Self {
        Self {
            source: "fn effect(uv: vec2<f32>) -> vec4<f32> {\n    return source(uv);\n}".into(),
            images: vec!["source".into()],
            controls: vec![],
        }
    }
}
impl Shader {
    pub fn program(
        &self,
        format: fold_platform::format::Format,
        time: f64,
        progress: f64,
    ) -> Result<fold_render::shader::Program, String> {
        fold_platform::effects::Interface {
            version: 1,
            images: self.images.clone(),
            controls: self.controls.clone(),
            extensions: Default::default(),
        }
        .validate()?;
        if self.controls.len() > 32 {
            return Err("Shader supports at most 32 controls".into());
        }
        let mut source = String::new();
        for (i, control) in self.controls.iter().enumerate() {
            if control.id.starts_with("fold_")
                || !control.id.bytes().enumerate().all(|(i, c)| {
                    c.is_ascii_alphabetic() || c == b'_' || (i > 0 && c.is_ascii_digit())
                })
            {
                return Err("Shader control identities must be WGSL identifiers".into());
            }
            let (kind, select) = match control.default.len() {
                1 => ("f32", ".x"),
                2 => ("vec2<f32>", ".xy"),
                3 => ("vec3<f32>", ".xyz"),
                _ => ("vec4<f32>", ""),
            };
            source.push_str(&format!(
                "fn {}()->{} {{ return fold_params.values[{i}]{select}; }}\n",
                control.id, kind
            ));
        }
        source.push_str(&self.source);
        let program = fold_render::shader::Program {
            source,
            images: self.images.clone(),
            values: self
                .controls
                .iter()
                .map(|c| {
                    let mut value = [0.; 4];
                    for (i, v) in c.default.iter().enumerate() {
                        value[i] = *v as f32;
                    }
                    value
                })
                .collect(),
            logical_size: format.display_size().map(|v| v as f32),
            time: time as f32,
            progress: progress as f32,
        };
        program.validate()?;
        Ok(program)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.program(fold_platform::format::Format::square([1920, 1080]), 0., 0.)
            .map(|_| ())
    }
}

/// Explicit interface edits preserve identities by name and optional renames.
/// Removal/type replacement must be listed, including disconnected ports.
pub fn migrate(
    node: &mut crate::Node,
    mut next: Shader,
    renames: &std::collections::BTreeMap<String, String>,
    remove: &[String],
) -> Result<(), String> {
    let crate::Parameters::Shader { settings: old } = &node.parameters else {
        return Err("Select a Shader node".into());
    };
    next.validate()?;
    let old_ids: std::collections::BTreeSet<_> = old
        .images
        .iter()
        .chain(old.controls.iter().map(|c| &c.id))
        .collect();
    if renames
        .keys()
        .chain(remove.iter())
        .any(|id| !old_ids.contains(id))
    {
        return Err("Migration names an unknown old port".into());
    }
    let target = |id: &String| renames.get(id).unwrap_or(id).clone();
    let mut used = std::collections::BTreeSet::new();
    let mut inputs = vec![None; next.images.len()];
    let mut animation_names = std::collections::BTreeMap::new();
    for (i, id) in old.images.iter().enumerate() {
        if remove.contains(id) {
            continue;
        }
        let name = target(id);
        let slot = next
            .images
            .iter()
            .position(|n| n == &name)
            .ok_or("Removing an image input requires an explicit removal")?;
        if !used.insert(name) {
            return Err("Migration collides with another port".into());
        }
        inputs[slot] = node.inputs[i].clone();
    }
    for control in &old.controls {
        let prefix = format!("{}.", control.id);
        for path in node
            .animation
            .bindings()
            .keys()
            .filter(|path| path.starts_with(&prefix))
        {
            animation_names.insert(path.clone(), None);
        }
        if remove.contains(&control.id) {
            continue;
        }
        let name = target(&control.id);
        let replacement = next
            .controls
            .iter_mut()
            .find(|c| c.id == name)
            .ok_or("Removing a control requires an explicit removal")?;
        if !used.insert(name.clone()) || replacement.default.len() != control.default.len() {
            return Err(
                "Control type replacement requires explicit removal before adding its new type"
                    .into(),
            );
        }
        replacement.default = control.default.clone();
        for path in node.animation.bindings().keys() {
            if let Some(component) = path.strip_prefix(&prefix) {
                animation_names.insert(path.clone(), Some(format!("{name}.{component}")));
            }
        }
    }
    let animation = node.animation.remap(&animation_names)?;
    node.inputs = inputs;
    node.animation = animation;
    node.parameters = crate::Parameters::Shader { settings: next };
    Ok(())
}

impl crate::Composite {
    /// Atomically migrate the node and its promoted property bindings.
    pub fn migrate_shader(
        &mut self,
        node: fold_foundation::ObjectId,
        settings: Shader,
        renames: &std::collections::BTreeMap<String, String>,
        remove: &[String],
    ) -> Result<(), String> {
        let mut candidate = self.clone();
        migrate(candidate.node_mut(node)?, settings, renames, remove)?;
        if candidate.extensions.contains_key(crate::reusable::KEY) {
            let mut definition = candidate.reusable()?;
            for binding in definition.bindings.iter_mut().filter(|b| b.node == node) {
                if remove.contains(&binding.property) {
                    return Err(
                        "Unpublish a promoted control before removing its shader port".into(),
                    );
                }
                if let Some(name) = renames.get(&binding.property) {
                    binding.property = name.clone();
                }
            }
            candidate.extensions.insert(
                crate::reusable::KEY.into(),
                serde_json::to_value(definition).map_err(|e| e.to_string())?,
            );
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
}

/// Uniform values and labels do not change the generated program interface.
#[cfg(feature = "ui")]
pub(crate) fn requires_validation(
    before: Option<&crate::Composite>,
    after: &crate::Composite,
) -> bool {
    after.nodes.iter().any(|node| {
        let crate::Parameters::Shader { settings } = &node.parameters else {
            return false;
        };
        let old = before.and_then(|graph| graph.node(node.id).ok());
        let Some(crate::Parameters::Shader { settings: old }) = old.map(|node| &node.parameters)
        else {
            return true;
        };
        old.source != settings.source
            || old.images != settings.images
            || !old
                .controls
                .iter()
                .map(|c| (&c.id, c.default.len()))
                .eq(settings.controls.iter().map(|c| (&c.id, c.default.len())))
    })
}
