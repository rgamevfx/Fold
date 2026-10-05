//! World-space constraints and explicit offsets without changing layer order.
use super::*;
use crate::{
    geometry::{self, Element, Geometry, IDENTITY, inverse, multiply},
    scene::{ConstraintKind, MaskSpace},
};
impl Evaluator<'_> {
    pub fn authored_matrix(&mut self, id: ObjectId) -> Result<[f64; 6], String> {
        let object = self
            .motion
            .scene
            .as_ref()
            .and_then(|s| s.objects.iter().find(|o| o.id == id))
            .ok_or("missing object")?;
        let Some(id) = object.transform else {
            return Ok(IDENTITY);
        };
        let node = self.motion.graph.node(id)?;
        let mut root = Self::for_graph(self.motion, &self.motion.graph, self.time)?;
        root.object_path = self.object_path.clone();
        root.current_object = self.current_object;
        root.budget = std::mem::take(&mut self.budget);
        let result = (|| {
            Ok(geometry::transform(
                root.vector(node, "translation")?,
                root.scalar(node, "rotation")?,
                root.vector(node, "scale")?,
                root.vector(node, "pivot")?,
            ))
        })();
        self.budget = root.budget;
        result
    }

    pub fn world_matrix(&mut self, id: ObjectId) -> Result<[f64; 6], String> {
        self.world(id, &mut BTreeSet::new())
    }
    fn world(&mut self, id: ObjectId, active: &mut BTreeSet<ObjectId>) -> Result<[f64; 6], String> {
        self.budget.spend()?;
        if active.len() > 64 || !active.insert(id) {
            return Err("cyclic transform relationship".into());
        }
        let object = self
            .motion
            .scene
            .as_ref()
            .and_then(|s| s.objects.iter().find(|o| o.id == id))
            .ok_or("missing transform object")?
            .clone();
        let parent = if let Some(id) = object.parent {
            self.world(id, active)?
        } else {
            IDENTITY
        };
        let mut matrix = multiply(parent, multiply(object.offset, self.authored_matrix(id)?));
        for original in &object.constraints {
            let mut c = original.clone();
            c.influence = crate::scene::animation::sample(
                &c.animation,
                "influence.0",
                c.influence,
                self.time,
            )
            .clamp(0., 1.);
            c.progress =
                crate::scene::animation::sample(&c.animation, "progress.0", c.progress, self.time)
                    .clamp(0., 1.);
            let target = self.world(c.target, active)?;
            let mut desired = multiply(target, c.offset);
            if c.kind == ConstraintKind::Path {
                let content = self.world_content(c.target)?;
                let path = geometry::path::Path::from_content(&content)?;
                let (point, angle) = path.sample(c.progress.clamp(0., 1.));
                desired = multiply(
                    geometry::transform(point, if c.orient { angle } else { 0. }, [1.; 2], [0.; 2]),
                    c.offset,
                );
            }
            let weight = c.influence;
            match c.kind {
                ConstraintKind::Follow | ConstraintKind::Path => {
                    matrix = geometry::blend_transform(matrix, desired, weight)?;
                }
                ConstraintKind::Position => {
                    for i in 4..6 {
                        matrix[i] += (desired[i] - matrix[i]) * weight;
                    }
                }
                ConstraintKind::Rotation | ConstraintKind::Aim => {
                    let old = matrix[1].atan2(matrix[0]);
                    let angle = if c.kind == ConstraintKind::Aim {
                        (target[5] - matrix[5]).atan2(target[4] - matrix[4])
                            + c.offset[1].atan2(c.offset[0])
                    } else {
                        desired[1].atan2(desired[0])
                    };
                    let delta = (angle - old + std::f64::consts::PI)
                        .rem_euclid(std::f64::consts::TAU)
                        - std::f64::consts::PI;
                    let rotation = geometry::transform(
                        [0.; 2],
                        (delta * weight).to_degrees(),
                        [1.; 2],
                        [0.; 2],
                    );
                    let position = [matrix[4], matrix[5]];
                    matrix = multiply(rotation, matrix);
                    matrix[4] = position[0];
                    matrix[5] = position[1];
                }
                ConstraintKind::Scale => {
                    for i in [0, 2] {
                        let old = matrix[i].hypot(matrix[i + 1]);
                        let new = desired[i].hypot(desired[i + 1]);
                        if old <= 1e-12 {
                            return Err("cannot constrain singular scale".into());
                        }
                        let factor = 1. + (new / old - 1.) * weight;
                        matrix[i] *= factor;
                        matrix[i + 1] *= factor;
                    }
                }
            }
        }
        active.remove(&id);
        Ok(matrix)
    }
    pub fn world_content(&mut self, id: ObjectId) -> Result<Content, String> {
        let parent = self
            .motion
            .scene
            .as_ref()
            .and_then(|s| s.objects.iter().find(|o| o.id == id))
            .ok_or("missing source")?
            .parent;
        let matrix = if let Some(parent) = parent {
            self.world_matrix(parent)?
        } else {
            IDENTITY
        };
        let Value::Content(content) = self.object(id)? else {
            return Err("source is not content".into());
        };
        Ok(wrap(content, matrix))
    }
    pub(super) fn scene_effects(
        &mut self,
        id: ObjectId,
        content: Content,
    ) -> Result<Content, String> {
        let mut object = self
            .motion
            .scene
            .as_ref()
            .and_then(|s| s.objects.iter().find(|o| o.id == id))
            .ok_or("missing object")?
            .clone();
        object.opacity = crate::scene::animation::sample(
            &object.animation,
            "scene.opacity.0",
            object.opacity,
            self.time,
        )
        .clamp(0., 1.);
        let parent = if let Some(parent) = object.parent {
            self.world_matrix(parent)?
        } else {
            IDENTITY
        };
        let correction = if object.constraints.is_empty() {
            object.offset
        } else {
            multiply(
                multiply(inverse(parent)?, self.world_matrix(id)?),
                inverse(self.authored_matrix(id)?)?,
            )
        };
        if correction == IDENTITY
            && object.opacity == 1.
            && !object.isolated
            && object.masks.is_empty()
        {
            return Ok(content);
        }
        let mut result = Element::new(id_seed(id), Geometry::Group(content));
        result.transform = correction;
        // Coverage is stored in parent coordinates alongside the corrected content.
        let mut wrapper = Element::new(id_seed(id), Geometry::Group(Arc::new(vec![result])));
        let mut masks = vec![];
        for original in &object.masks {
            let mut mask = original.clone();
            mask.opacity = crate::scene::animation::sample(
                &mask.animation,
                "opacity.0",
                mask.opacity,
                self.time,
            )
            .clamp(0., 1.);
            mask.feather = crate::scene::animation::sample(
                &mask.animation,
                "feather.0",
                mask.feather,
                self.time,
            )
            .clamp(0., 256.);
            let content = match mask.space {
                MaskSpace::World => wrap(self.world_content(mask.source)?, inverse(parent)?),
                MaskSpace::Local => {
                    let Value::Content(source) = self.object(mask.source)? else {
                        return Err("mask requires content".into());
                    };
                    wrap(source, multiply(inverse(parent)?, self.world_matrix(id)?))
                }
            };
            masks.push(geometry::render::Coverage {
                content,
                operation: mask.operation,
                invert: mask.invert,
                opacity: mask.opacity,
                feather: mask.feather,
            });
        }
        if object.isolated || !masks.is_empty() {
            wrapper.effects = Some(geometry::render::Effects {
                opacity: object.opacity,
                masks,
            });
        } else {
            wrapper.opacity = object.opacity;
        }
        Ok(Arc::new(vec![wrapper]))
    }
}
fn id_seed(id: ObjectId) -> u64 {
    serde_json::to_string(&id)
        .unwrap()
        .bytes()
        .fold(0u64, |v, b| v.wrapping_mul(31).wrapping_add(b as u64))
}
fn wrap(content: Content, matrix: [f64; 6]) -> Content {
    let mut e = Element::new(0, Geometry::Group(content));
    e.transform = matrix;
    Arc::new(vec![e])
}
