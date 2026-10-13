//! Camera-space selection and world-space movement over evaluated native state.
use crate::{
    editing, evaluation,
    model::{Content, Projection, Scene},
};
use fold_foundation::{ObjectId, Time, affine::Affine};

pub(crate) fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(crate) fn mul(a: [f64; 3], b: f64) -> [f64; 3] {
    a.map(|v| v * b)
}
pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(v: [f64; 3]) -> Result<[f64; 3], String> {
    let length = dot(v, v).sqrt();
    if !length.is_finite() || length < 1e-12 {
        return Err("Invalid 3D view direction".into());
    }
    Ok(mul(v, length.recip()))
}
pub fn world(scene: &Scene, id: ObjectId, time: Time) -> Result<Affine, String> {
    let mut chain = vec![];
    let mut next = Some(id);
    while let Some(id) = next {
        if chain.len() > 128 {
            return Err("3D hierarchy depth exceeds 128".into());
        }
        chain.push(evaluation::local(
            &scene.objects.get(&id).ok_or("Missing 3D object")?.transform,
            time,
        )?);
        next = editing::placement(scene, id)?.parent;
    }
    Ok(chain
        .into_iter()
        .rev()
        .fold(Affine::IDENTITY, |parent, local| parent.compose(local)))
}
#[derive(Clone, Copy)]
pub struct View {
    position: [f64; 3],
    forward: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
    projection: Projection,
    aspect: f64,
    near: f64,
    far: f64,
}
impl View {
    pub fn new(scene: &Scene, time: Time, dimensions: [u32; 2]) -> Result<Self, String> {
        if dimensions.contains(&0) {
            return Err("Invalid 3D view dimensions".into());
        }
        let camera = scene
            .objects
            .get(&scene.active_camera)
            .ok_or("Missing active camera")?;
        let Content::Camera {
            target,
            up,
            projection,
            near,
            far,
            ..
        } = camera.content
        else {
            return Err("Invalid active camera".into());
        };
        let matrix = world(scene, scene.active_camera, time)?;
        let position = matrix.point([0.; 3]);
        let forward = unit(sub(matrix.point(target.map(f64::from)), position))?;
        let right = unit(cross(forward, matrix.vector(up.map(f64::from))))?;
        let up = cross(right, forward);
        Ok(Self {
            position,
            forward,
            right,
            up,
            projection,
            aspect: dimensions[0] as f64 / dimensions[1] as f64,
            near: near as f64,
            far: far as f64,
        })
    }
    fn half_height(self, depth: f64) -> f64 {
        match self.projection {
            Projection::Perspective { vertical_fov } => depth * (vertical_fov as f64 * 0.5).tan(),
            Projection::Orthographic { vertical_size } => vertical_size as f64 * 0.5,
        }
    }
    /// Normalized top-left image coordinates, before viewer pan/zoom.
    pub fn project(self, point: [f64; 3]) -> Option<[f64; 2]> {
        let relative = sub(point, self.position);
        let depth = dot(relative, self.forward);
        if depth < self.near || depth > self.far {
            return None;
        }
        let height = self.half_height(depth);
        Some([
            0.5 + dot(relative, self.right) / (2. * height * self.aspect),
            0.5 - dot(relative, self.up) / (2. * height),
        ])
    }
    pub fn units_per_height(self, point: [f64; 3]) -> f64 {
        2. * self.half_height(dot(sub(point, self.position), self.forward))
    }
    fn ray(self, point: [f64; 2]) -> Result<([f64; 3], [f64; 3]), String> {
        if point.iter().any(|v| !v.is_finite()) {
            return Err("Invalid 3D pointer".into());
        }
        let offset = add(
            mul(
                self.right,
                (2. * point[0] - 1.) * self.half_height(1.) * self.aspect,
            ),
            mul(self.up, (1. - 2. * point[1]) * self.half_height(1.)),
        );
        Ok(match self.projection {
            Projection::Perspective { .. } => (self.position, unit(add(self.forward, offset))?),
            Projection::Orthographic { .. } => (add(self.position, offset), self.forward),
        })
    }
    /// Closest point on a world axis to the pointer ray. Parallel views reject.
    pub fn axis(self, pointer: [f64; 2], origin: [f64; 3], axis: [f64; 3]) -> Option<f64> {
        let (eye, ray) = self.ray(pointer).ok()?;
        let a = dot(axis, ray);
        let denominator = 1. - a * a;
        if denominator < 1e-6 {
            return None;
        }
        let relative = sub(origin, eye);
        Some((a * dot(ray, relative) - dot(axis, relative)) / denominator)
    }
    /// Move in the camera-facing plane through the selected origin.
    pub fn plane(self, pointer: [f64; 2], origin: [f64; 3]) -> Option<[f64; 3]> {
        let (eye, ray) = self.ray(pointer).ok()?;
        let denominator = dot(ray, self.forward);
        if denominator.abs() < 1e-9 {
            return None;
        }
        Some(add(
            eye,
            mul(ray, dot(sub(origin, eye), self.forward) / denominator),
        ))
    }
    /// Intersect a local coordinate plane, retaining parent shear and scale.
    pub fn local_plane(self, pointer: [f64; 2], matrix: Affine, axis: usize) -> Option<[f64; 3]> {
        if axis > 2 {
            return None;
        }
        let (eye, ray) = self.ray(pointer).ok()?;
        let inverse = matrix.inverse().ok()?;
        let eye = inverse.point(eye);
        let ray = inverse.vector(ray);
        if ray[axis].abs() < 1e-9 {
            return None;
        }
        let t = -eye[axis] / ray[axis];
        // A plane through the camera has no usable pointer intersection. Small
        // raster rounding must not make its edge-on ring steal another handle.
        (t > 1e-6).then(|| add(eye, mul(ray, t)))
    }
    /// Test the exact evaluated triangles used by rendering. No GPU readback.
    pub fn pick(
        self,
        scene: &Scene,
        time: Time,
        pointer: [f64; 2],
    ) -> Result<Option<ObjectId>, String> {
        let (eye, ray) = self.ray(pointer)?;
        let (evaluated, _) = scene.evaluate(
            fold_foundation::DocumentId::new(),
            Default::default(),
            time,
            [scene.info.width, scene.info.height],
            &|| Ok(()),
        )?;
        let mut nearest = self.far;
        let mut hit = None;
        for instance in evaluated.instances {
            if evaluated.materials[&instance.material].color[3] <= 0. {
                continue;
            }
            let matrix = instance
                .transform
                .affine
                .ok_or("Missing evaluated transform")?;
            let inverse = matrix.inverse()?;
            let origin = inverse.point(eye);
            let direction = inverse.vector(ray);
            let mesh = &evaluated.meshes[&instance.mesh];
            for indices in mesh.indices.chunks_exact(3) {
                let [a, b, c] = [indices[0], indices[1], indices[2]]
                    .map(|i| mesh.positions[i as usize].map(f64::from));
                let ab = sub(b, a);
                let ac = sub(c, a);
                let p = cross(direction, ac);
                let determinant = dot(ab, p);
                if determinant.abs() < 1e-12 {
                    continue;
                }
                let offset = sub(origin, a);
                let u = dot(offset, p) / determinant;
                if !(0. ..=1.).contains(&u) {
                    continue;
                }
                let q = cross(offset, ab);
                let v = dot(direction, q) / determinant;
                if v < 0. || u + v > 1. {
                    continue;
                }
                let t = dot(ac, q) / determinant;
                // Clip by forward depth, as the renderer does, not ray length.
                let depth = t * dot(ray, self.forward);
                if depth >= self.near && depth < nearest {
                    nearest = depth;
                    hit = Some(instance.id);
                }
            }
        }
        Ok(hit)
    }
}
