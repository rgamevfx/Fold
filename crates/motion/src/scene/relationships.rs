//! Explicit scene relationships; ordering and transformation are independent contracts.
use fold_foundation::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintKind {
    Follow,
    Position,
    Rotation,
    Scale,
    Aim,
    Path,
}
impl ConstraintKind {
    pub const ALL: [Self; 6] = [
        Self::Follow,
        Self::Position,
        Self::Rotation,
        Self::Scale,
        Self::Aim,
        Self::Path,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Follow => "Follow Transform",
            Self::Position => "Copy Position",
            Self::Rotation => "Copy Rotation",
            Self::Scale => "Copy Scale",
            Self::Aim => "Aim",
            Self::Path => "Follow Path",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub id: ObjectId,
    pub target: ObjectId,
    pub kind: ConstraintKind,
    pub influence: f64,
    #[serde(default)]
    pub animation: super::animation::Channels,
    pub offset: [f64; 6],
    pub progress: f64,
    pub orient: bool,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
impl Constraint {
    pub fn new(target: ObjectId, kind: ConstraintKind) -> Self {
        Self {
            id: ObjectId::new(),
            target,
            kind,
            influence: 1.,
            animation: Default::default(),
            offset: crate::geometry::IDENTITY,
            progress: 0.,
            orient: true,
            extensions: Default::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskSpace {
    Local,
    World,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskOperation {
    Intersect,
    Add,
    Subtract,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mask {
    pub id: ObjectId,
    pub source: ObjectId,
    pub space: MaskSpace,
    pub operation: MaskOperation,
    pub invert: bool,
    pub opacity: f64,
    #[serde(default)]
    pub animation: super::animation::Channels,
    pub feather: f64,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
impl Mask {
    pub fn new(source: ObjectId, space: MaskSpace) -> Self {
        Self {
            id: ObjectId::new(),
            source,
            space,
            operation: MaskOperation::Intersect,
            invert: false,
            opacity: 1.,
            animation: Default::default(),
            feather: 0.,
            extensions: Default::default(),
        }
    }
}
pub(super) fn validate(scene: &super::Scene) -> Result<(), String> {
    fn visit(
        scene: &super::Scene,
        id: ObjectId,
        active: &mut Vec<ObjectId>,
        done: &mut std::collections::BTreeSet<ObjectId>,
    ) -> Result<(), String> {
        if done.contains(&id) {
            return Ok(());
        }
        if active.contains(&id) || active.len() > 64 {
            return Err("cyclic transform relationship".into());
        }
        let object = scene
            .objects
            .iter()
            .find(|o| o.id == id)
            .ok_or("missing transform target")?;
        active.push(id);
        for target in object
            .parent
            .iter()
            .copied()
            .chain(object.constraints.iter().map(|c| c.target))
        {
            visit(scene, target, active, done)?;
        }
        active.pop();
        done.insert(id);
        Ok(())
    }
    let mut done = Default::default();
    let mut identities: std::collections::BTreeSet<_> =
        scene.objects.iter().map(|o| o.id).collect();
    for object in &scene.objects {
        for id in object
            .constraints
            .iter()
            .map(|c| c.id)
            .chain(object.masks.iter().map(|m| m.id))
        {
            if !identities.insert(id) {
                return Err("duplicate relationship identity".into());
            }
        }
        super::animation::validate(&object.animation, &["scene.opacity.0"])?;
        if !object.offset.iter().all(|v| v.is_finite()) || !(0. ..=1.).contains(&object.opacity) {
            return Err("invalid scene transform or opacity".into());
        }
        for c in &object.constraints {
            super::animation::validate(&c.animation, &["influence.0", "progress.0"])?;
            if !(0. ..=1.).contains(&c.influence)
                || !c.progress.is_finite()
                || !c.offset.iter().all(|v| v.is_finite())
            {
                return Err("invalid constraint controls".into());
            }
        }
        for mask in &object.masks {
            super::animation::validate(&mask.animation, &["opacity.0", "feather.0"])?;
            if !scene.objects.iter().any(|o| o.id == mask.source)
                || !(0. ..=1.).contains(&mask.opacity)
                || !(0. ..=256.).contains(&mask.feather)
            {
                return Err("invalid mask source or controls".into());
            }
        }
        visit(scene, object.id, &mut vec![], &mut done)?;
    }
    Ok(())
}
