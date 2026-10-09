//! Registered native geometry and value functions for Rhai.
use super::super::{
    Geometry, GeometryData,
    attributes::{Attributes, Domain, stable_id, vector},
    mesh,
};
use super::{Result, Work};
use crate::types::{Literal, Space, Unit};
use rhai::{Array, Dynamic, Engine};
use std::sync::Arc;
#[derive(Clone, Debug)]
struct Vector([f64; 3]);
#[derive(Clone, Debug)]
struct Quaternion([f64; 4]);
pub(super) fn empty(id: u64) -> Geometry {
    Geometry {
        attributes: Attributes::default(),
        id,
        space: Space::World,
        unit: Unit::Meters,
        appearance: Default::default(),
        data: GeometryData::Mesh(Arc::new(mesh::Mesh {
            source: None,
            vertex_ids: vec![],
            face_ids: vec![],
            prepared: mesh::Prepared {
                mesh: Arc::new(fold_render::spatial::Mesh {
                    positions: vec![],
                    normals: vec![],
                    indices: vec![],
                }),
                uv: vec![],
                regions: Default::default(),
                source_vertices: vec![],
                source_geometry: None,
            },
        })),
    }
}
fn index(i: i64) -> Result<usize> {
    usize::try_from(i).map_err(|_| "Index must be nonnegative".into())
}
fn position(v: Vector) -> Result<[f32; 3]> {
    if v.0.iter().any(|x| !x.is_finite() || x.abs() > 1e6) {
        return Err("Position must be finite and within supported extent".into());
    }
    Ok(v.0.map(|x| x as f32))
}
fn mesh_mut(g: &mut Geometry) -> Result<&mut mesh::Mesh> {
    match &mut g.data {
        GeometryData::Mesh(m) => Ok(Arc::make_mut(m)),
        _ => Err("Operation requires a mesh".into()),
    }
}
fn from_mesh(
    work: &Work,
    mesh: Arc<fold_render::spatial::Mesh>,
    uv: Vec<[f32; 2]>,
) -> Result<Geometry> {
    work.reserve(mesh.positions.len() + mesh.indices.len() / 3)?;
    let mut g = empty(work.identity());
    let id = g.id;
    let m = mesh_mut(&mut g)?;
    m.vertex_ids = (0..mesh.positions.len())
        .map(|i| stable_id(id, i as u64))
        .collect();
    m.face_ids = (0..mesh.indices.len() / 3)
        .map(|i| stable_id(id, 0x100000000 + i as u64))
        .collect();
    m.prepared.mesh = mesh;
    m.prepared.uv = uv;
    Ok(g)
}
fn literal(value: Dynamic, name: &str) -> Result<Literal> {
    if value.is::<Vector>() {
        return Ok(vector(
            value.cast::<Vector>().0,
            if name == "P" {
                Unit::Meters
            } else {
                Unit::Unitless
            },
        ));
    }
    if value.is::<Quaternion>() {
        return Ok(Literal::Rotation {
            value: value.cast::<Quaternion>().0,
            space: Space::World,
        });
    }
    if value.is::<f64>() {
        return Ok(Literal::Scalar {
            value: value.cast(),
        });
    }
    if value.is::<i64>() {
        return Ok(Literal::Integer {
            value: value.cast(),
        });
    }
    if value.is::<bool>() {
        return Ok(Literal::Bool {
            value: value.cast(),
        });
    }
    if value.is::<rhai::ImmutableString>() {
        return Ok(Literal::Text {
            value: value.cast::<rhai::ImmutableString>().to_string(),
        });
    }
    Err("Attributes support float, integer, boolean, string, vec3 and quaternion".into())
}
fn dynamic(value: Literal) -> Result<Dynamic> {
    Ok(match value {
        Literal::Vector { value, .. } => {
            let components: [f64; 3] = value
                .try_into()
                .map_err(|_| "Script vectors require three components")?;
            Dynamic::from(Vector(components))
        }
        Literal::Rotation { value, .. } => Dynamic::from(Quaternion(value)),
        Literal::Scalar { value } => Dynamic::from(value),
        Literal::Integer { value } => Dynamic::from(value),
        Literal::Bool { value } => Dynamic::from(value),
        Literal::Text { value } => Dynamic::from(value),
        _ => return Err("Unsupported script attribute type".into()),
    })
}
pub(super) fn engine(work: Work) -> Engine {
    let mut engine = Engine::new();
    // Cached source must never embed geometry generated under another invocation.
    engine.set_optimization_level(rhai::OptimizationLevel::None);
    engine
        .set_max_operations(1_000_000)
        .set_max_call_levels(32)
        .set_max_expr_depths(64, 64)
        .set_max_array_size(4096)
        .set_max_map_size(256)
        .set_max_string_size(65_536)
        .set_max_variables(256)
        .set_max_functions(256)
        .set_max_modules(1);
    // No file-backed module resolver or host IO is exposed.
    engine.set_module_resolver(rhai::module_resolvers::StaticModuleResolver::new());
    engine.on_print(|_| {});
    engine.on_debug(|_, _, _| {});
    let progress = work.clone();
    engine.on_progress(move |_| progress.tick().err().map(|e| Dynamic::from(e.to_string())));
    engine.register_type_with_name::<Geometry>("Geometry");
    engine.register_type_with_name::<Vector>("vec3");
    engine.register_type_with_name::<Quaternion>("quaternion");
    engine.register_fn("vec3", |x: f64, y: f64, z: f64| Vector([x, y, z]));
    for (name, axis) in [("x", 0), ("y", 1), ("z", 2), ("r", 0), ("g", 1), ("b", 2)] {
        engine.register_get_set(
            name,
            move |v: &mut Vector| v.0[axis],
            move |v: &mut Vector, x: f64| v.0[axis] = x,
        );
    }
    engine.register_fn("+", |a: Vector, b: Vector| {
        Vector(std::array::from_fn(|i| a.0[i] + b.0[i]))
    });
    engine.register_fn("-", |a: Vector, b: Vector| {
        Vector(std::array::from_fn(|i| a.0[i] - b.0[i]))
    });
    engine.register_fn("*", |a: Vector, b: f64| Vector(a.0.map(|x| x * b)));
    engine.register_fn("/", |a: Vector, b: f64| Vector(a.0.map(|x| x / b)));
    engine.register_fn("length", |v: Vector| {
        v.0.iter().map(|v| v * v).sum::<f64>().sqrt()
    });
    engine.register_fn("normalize", |v: Vector| -> Result<Vector> {
        let l = v.0.iter().map(|v| v * v).sum::<f64>().sqrt();
        if l < 1e-12 || !l.is_finite() {
            return Err("Cannot normalize zero/nonfinite vector".into());
        }
        Ok(Vector(v.0.map(|v| v / l)))
    });
    engine.register_fn("orient", |axis: Vector, angle: f64| -> Result<Quaternion> {
        let l = axis.0.iter().map(|v| v * v).sum::<f64>().sqrt();
        if l < 1e-12 || !l.is_finite() || !angle.is_finite() {
            return Err("Invalid rotation axis/angle".into());
        }
        let (s, c) = (angle * 0.5).sin_cos();
        Ok(Quaternion([
            axis.0[0] / l * s,
            axis.0[1] / l * s,
            axis.0[2] / l * s,
            c,
        ]))
    });
    engine.register_fn("random", |id: i64, seed: i64| {
        ((stable_id(seed as u64, id as u64) >> 11) as f64) / (1u64 << 53) as f64
    });
    engine.register_fn("dot", |a: Vector, b: Vector| {
        a.0.iter().zip(b.0).map(|(a, b)| a * b).sum::<f64>()
    });
    engine.register_fn("cross", |a: Vector, b: Vector| {
        Vector([
            a.0[1] * b.0[2] - a.0[2] * b.0[1],
            a.0[2] * b.0[0] - a.0[0] * b.0[2],
            a.0[0] * b.0[1] - a.0[1] * b.0[0],
        ])
    });
    engine.register_fn("lerp", |a: Vector, b: Vector, t: f64| {
        Vector(std::array::from_fn(|i| a.0[i] + (b.0[i] - a.0[i]) * t))
    });
    engine.register_fn(
        "slerp",
        |a: Quaternion, b: Quaternion, t: f64| -> Result<Quaternion> {
            if !t.is_finite() {
                return Err("Interpolation requires finite amount".into());
            }
            Ok(Quaternion(fold_foundation::rotation::slerp(a.0, b.0, t)))
        },
    );
    engine.register_fn("noise", |p: Vector, seed: i64| -> Result<f64> {
        if p.0.iter().any(|x| !x.is_finite() || x.abs() > 1e6) {
            return Err("Noise coordinates exceed supported extent".into());
        }
        let cell = p.0.map(|x| x.floor() as i64);
        let f = std::array::from_fn::<_, 3, _>(|i| {
            let t = p.0[i] - p.0[i].floor();
            t * t * t * (t * (t * 6. - 15.) + 10.)
        });
        let mut value = 0.;
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    let bits = stable_id(
                        stable_id(
                            stable_id(seed as u64, (cell[0] + x) as u64),
                            (cell[1] + y) as u64,
                        ),
                        (cell[2] + z) as u64,
                    );
                    let n = (bits >> 11) as f64 / (1u64 << 53) as f64;
                    let weight = if x == 0 { 1. - f[0] } else { f[0] };
                    let weight = weight * if y == 0 { 1. - f[1] } else { f[1] };
                    let weight = weight * if z == 0 { 1. - f[2] } else { f[2] };
                    value += n * weight;
                }
            }
        }
        Ok(value * 2. - 1.)
    });
    let w = work.clone();
    engine.register_fn("geometry", move || empty(w.identity()));
    let w = work.clone();
    engine.register_fn("sphere", move |radius: f64| -> Result<Geometry> {
        w.tick()?;
        if !(1e-6..=1e6).contains(&radius) {
            return Err("Invalid sphere radius".into());
        }
        let (m, uv) = fold_render::spatial::primitives::sphere_mesh(radius as f32)
            .map_err(|e| format!("Sphere: {e:?}"))?;
        from_mesh(&w, m, uv)
    });
    let w = work.clone();
    engine.register_fn(
        "grid",
        move |columns: i64, rows: i64, spacing: f64| -> Result<Geometry> {
            let columns = index(columns)?;
            let rows = index(rows)?;
            let count = columns.checked_mul(rows).ok_or("Grid count overflow")?;
            w.reserve(
                count.saturating_add(
                    columns
                        .saturating_sub(1)
                        .saturating_mul(rows.saturating_sub(1))
                        .saturating_mul(2),
                ),
            )?;
            let mut g = empty(w.identity());
            g.data = GeometryData::Mesh(Arc::new(mesh::grid(
                columns,
                rows,
                [spacing; 2],
                [0.; 3],
                g.id,
                &mut || w.tick().map_err(|e| e.to_string()),
            )?));
            Ok(g)
        },
    );
    engine.register_fn("count", |g: &mut Geometry, domain: &str| -> Result<i64> {
        Ok(g.count(Domain::parse(domain)?)? as i64)
    });
    engine.register_fn("point", |g: &mut Geometry, i: i64| -> Result<Vector> {
        Ok(Vector(g.position(Domain::Point, index(i)?)?))
    });
    engine.register_fn(
        "element_id",
        |g: &mut Geometry, domain: &str, i: i64| -> Result<i64> {
            Ok(g.element_id(Domain::parse(domain)?, index(i)?)? as i64)
        },
    );
    engine.register_fn(
        "attribute",
        |g: &mut Geometry, domain: &str, name: &str, i: i64| -> Result<Dynamic> {
            dynamic(g.read_attribute(Domain::parse(domain)?, name, index(i)?)?)
        },
    );
    let w = work.clone();
    engine.register_fn(
        "set_attribute",
        move |g: &mut Geometry, domain: &str, name: &str, i: i64, value: Dynamic| -> Result<()> {
            w.edit(g)?;
            let d = Domain::parse(domain)?;
            let intrinsic = (d == Domain::Point && matches!(name, "P" | "N"))
                || (d == Domain::Instance && matches!(name, "P" | "orient" | "pscale"));
            if g.attributes.get(d, name).is_none() && !intrinsic {
                w.reserve(g.count(d)?)?;
            }
            let value = literal(value, name)?;
            w.reserve(super::super::attributes::value_bytes(&value).div_ceil(128))?;
            g.set_attribute_element(Domain::parse(domain)?, name, index(i)?, value)?;
            g.id = stable_id(g.id, w.seed);
            Ok(())
        },
    );
    let w = work.clone();
    engine.register_fn(
        "set_point",
        move |g: &mut Geometry, i: i64, p: Vector| -> Result<()> {
            w.edit(g)?;
            let i = index(i)?;
            if i >= g.count(Domain::Point)? {
                return Err("Point index out of range".into());
            }
            Arc::make_mut(&mut mesh_mut(g)?.prepared.mesh).positions[i] = position(p)?;
            g.id = stable_id(g.id, w.seed);
            Ok(())
        },
    );
    let w = work.clone();
    engine.register_fn(
        "add_point",
        move |g: &mut Geometry, p: Vector| -> Result<i64> {
            w.edit(g)?;
            w.reserve(1)?;
            let p = position(p)?;
            let m = mesh_mut(g)?;
            let i = m.vertex_ids.len();
            m.vertex_ids.push(w.identity());
            m.prepared.uv.push([0.; 2]);
            // A generated point has no source-cap correspondence.
            m.prepared.source_vertices.clear();
            m.prepared.source_geometry = None;
            let mesh = Arc::make_mut(&mut m.prepared.mesh);
            mesh.positions.push(p);
            mesh.normals.push([0., 0., 1.]);
            for a in Arc::make_mut(&mut g.attributes.0) {
                if a.domain == Domain::Point {
                    let default = super::super::attributes::default_for(&a.name, &a.value_type)?;
                    a.values.push(default)
                }
            }
            g.id = stable_id(g.id, w.seed);
            Ok(i as i64)
        },
    );
    let w = work.clone();
    engine.register_fn(
        "add_triangle",
        move |g: &mut Geometry, a: i64, b: i64, c: i64| -> Result<i64> {
            w.edit(g)?;
            w.reserve(1)?;
            let before = g.clone();
            let m = mesh_mut(g)?;
            let face = [index(a)?, index(b)?, index(c)?];
            if face.iter().any(|i| *i >= m.vertex_ids.len()) || a == b || a == c || b == c {
                return Err("Invalid triangle points".into());
            }
            let index = m.face_ids.len();
            m.face_ids.push(w.identity());
            Arc::make_mut(&mut m.prepared.mesh)
                .indices
                .extend(face.map(|i| i as u32));
            remap_topology(&before, g, &w)?;
            g.id = stable_id(g.id, w.seed);
            Ok(index as i64)
        },
    );
    let w = work.clone();
    engine.register_fn(
        "remove_point",
        move |g: &mut Geometry, i: i64| -> Result<()> {
            w.edit(g)?;
            let i = index(i)?;
            if i >= g.count(Domain::Point)? {
                return Err("Point index out of range".into());
            }
            let before = g.clone();
            let m = mesh_mut(g)?;
            let mesh = Arc::make_mut(&mut m.prepared.mesh);
            mesh.positions.remove(i);
            mesh.normals.remove(i);
            m.prepared.uv.remove(i);
            if !m.prepared.source_vertices.is_empty() {
                m.prepared.source_vertices.remove(i);
            }
            m.vertex_ids.remove(i);
            let mut faces = vec![];
            let mut indices = vec![];
            for (f, id) in mesh.indices.chunks_exact(3).zip(&m.face_ids) {
                w.tick()?;
                if !f.contains(&(i as u32)) {
                    faces.push(*id);
                    indices.extend(f.iter().map(|v| if *v > i as u32 { v - 1 } else { *v }));
                }
            }
            mesh.indices = indices;
            m.face_ids = faces;
            m.prepared.regions.clear();
            remap_topology(&before, g, &w)?;
            g.id = stable_id(g.id, w.seed);
            Ok(())
        },
    );
    let w = work.clone();
    engine.register_fn(
        "nearpoints",
        move |g: &mut Geometry, p: Vector, radius: f64| -> Result<Array> {
            if radius < 0. || !radius.is_finite() {
                return Err("Radius must be finite and nonnegative".into());
            }
            let mut found = vec![];
            for i in 0..g.count(Domain::Point)? {
                w.tick()?;
                let q = g.position(Domain::Point, i)?;
                if q.iter()
                    .zip(p.0)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>()
                    <= radius * radius
                {
                    found.push(Dynamic::from(i as i64));
                }
            }
            Ok(found)
        },
    );
    let w = work.clone();
    engine.register_fn(
        "copy_to_points",
        move |prototype: Geometry, points: Geometry, attributes: &str| -> Result<Geometry> {
            w.reserve(
                points
                    .count(Domain::Point)?
                    .saturating_add(super::super::cloner::attribute_cost(&points, attributes)),
            )?;
            super::super::cloner::copy(
                Arc::new(prototype),
                Arc::new(points),
                attributes,
                w.identity(),
                &mut || w.tick().map_err(|e| e.to_string()),
            )
            .map_err(Into::into)
        },
    );
    let w = work.clone();
    engine.register_fn("box_mesh", move |size: Vector| -> Result<Geometry> {
        w.tick()?;
        let p = fold_render::spatial::primitives::box_mesh(position(size)?)
            .map_err(|e| format!("Box: {e:?}"))?;
        from_mesh(&w, p.mesh, p.uv)
    });
    let w = work.clone();
    engine.register_fn("compute_normals", move |g: &mut Geometry| -> Result<()> {
        w.edit(g)?;
        let m = mesh_mut(g)?;
        let mesh = Arc::make_mut(&mut m.prepared.mesh);
        let mut normals = vec![[0f64; 3]; mesh.positions.len()];
        for face in mesh.indices.chunks_exact(3) {
            w.tick()?;
            let a = mesh.positions[face[0] as usize].map(f64::from);
            let b = mesh.positions[face[1] as usize].map(f64::from);
            let c = mesh.positions[face[2] as usize].map(f64::from);
            let u = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
            let v = std::array::from_fn::<_, 3, _>(|i| c[i] - a[i]);
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            for index in face {
                for axis in 0..3 {
                    normals[*index as usize][axis] += n[axis];
                }
            }
        }
        for (out, n) in mesh.normals.iter_mut().zip(normals) {
            w.tick()?;
            let length = n.iter().map(|v| v * v).sum::<f64>().sqrt();
            *out = if length > 1e-12 {
                n.map(|v| (v / length) as f32)
            } else {
                [0., 0., 1.]
            };
        }
        g.id = stable_id(g.id, w.seed);
        Ok(())
    });
    engine
}

fn remap_topology(before: &Geometry, after: &mut Geometry, work: &Work) -> Result<()> {
    use super::super::attributes::default_for;
    for a in before.attributes.0.iter() {
        if a.domain == Domain::Detail {
            continue;
        }
        work.tick()?;
        let old = before.element_ids(a.domain)?;
        let new = after.element_ids(a.domain)?;
        work.reserve(new.len())?;
        let map: std::collections::BTreeMap<_, _> = old.into_iter().zip(&a.values).collect();
        let default = default_for(&a.name, &a.value_type)?;
        let mut values = Vec::with_capacity(new.len());
        for id in new {
            work.tick()?;
            values.push(
                map.get(&id)
                    .map_or_else(|| default.clone(), |v| (*v).clone()),
            );
        }
        after
            .attributes
            .put(a.domain, &a.name, a.value_type.clone(), values);
    }
    Ok(())
}
