//! Exact CPU measurements. No raster, camera projection or artist state access.
use super::{Failure, operation_budget};
use fold_foundation::{ObjectId, Time};
use fold_media::Cancel;
use fold_motion::{
    candidates::Handle,
    evaluation::{self, Geometry, GeometryData, Placement, Prepared},
    model::Construction,
};
use fold_project::{Document, Revision};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn run(
    epoch: ObjectId,
    document: &Document,
    revision: Revision,
    candidate: Option<Handle>,
    output: ObjectId,
    times: &[Time],
    cancel: &Cancel,
) -> Result<Value, Failure> {
    if times.is_empty() || times.len() > 64 {
        return Err(Failure::new("budget", "Observe requires 1..64 exact times"));
    }
    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
    let value = Construction::from_document(document).map_err(Failure::invalid)?;
    operation_budget(&value)?;
    let root = value
        .definition(value.root.definition)
        .map_err(Failure::invalid)?;
    if !root.outputs.iter().any(|o| o.id == output) {
        return Err(Failure::new(
            "missing_resource",
            "Output is not declared by the root asset",
        ));
    }
    let limits = evaluation::Limits {
        steps: 1_000_000 / times.len(),
        elements: 1_000_000 / times.len(),
    };
    let configuration = json!({"profile":"cpu_reference_exact","steps_per_sample":limits.steps,"elements_per_sample":limits.elements,"measurement_steps_per_sample":limits.elements,"samples":times.len(),"build":env!("FOLD_SOURCE_SHA256"),"target":env!("FOLD_BUILD_TARGET")});
    let mut provenance = json!({
        "epoch":epoch,"document":document.id,"revision":revision,"candidate":candidate,"output":output,
        "seed":value.seed,"quality":"cpu_reference_exact","camera":null,"output_size":null,"frames_available":false,
        "fingerprints":{"construction":digest(&serde_json::to_vec(&value).expect("construction JSON")),"assets":digest(b"[]"),"fonts":digest(b"[]"),"configuration":digest(&serde_json::to_vec(&configuration).expect("configuration JSON"))},
        "resource_inputs":{"assets":[],"fonts":[],"reason":"Qualified CPU operations do not resolve external resources"},
        "configuration":configuration
    });
    let prepared = Prepared::new_detailed(value)?;
    let mut samples = Vec::with_capacity(times.len());
    let mut fonts = BTreeMap::new();
    for &time in times {
        cancel.check().map_err(|e| Failure::new("cancelled", e))?;
        let result = prepared.evaluate_detailed(output, time, limits, &|| cancel.check())?;
        let mut measurement = Measurement {
            cancel,
            geometries: BTreeMap::new(),
            points: 0,
            placements: 0,
            glyphs: 0,
            fonts: BTreeMap::new(),
            remaining: limits.elements,
        };
        let result = measurement.value(&result)?;
        fonts.extend(measurement.fonts);
        samples.push(json!({"time":time,"asset_time":prepared.construction().root.clock.at(time).map_err(Failure::invalid)?,"value":result,
            "geometries":measurement.geometries,"counts":{"unique_geometries":measurement.geometries.len(),"unique_points":measurement.points,"placements":measurement.placements,"glyphs":measurement.glyphs},"exact":true}));
    }
    if !fonts.is_empty() {
        let fonts: Vec<_> = fonts.into_values().collect();
        provenance["fingerprints"]["fonts"] = json!(digest(
            &serde_json::to_vec(&fonts).expect("font provenance JSON")
        ));
        provenance["resource_inputs"]["fonts"] = json!(fonts);
        provenance["resource_inputs"]["reason"] =
            json!("Explicit packaged fonts; no external asset lookup");
    }
    Ok(json!({"provenance":provenance,"samples":samples}))
}
struct Measurement<'a> {
    cancel: &'a Cancel,
    geometries: BTreeMap<u64, Value>,
    points: usize,
    placements: usize,
    glyphs: usize,
    fonts: BTreeMap<String, Value>,
    remaining: usize,
}
impl Measurement<'_> {
    fn step(&mut self) -> Result<(), Failure> {
        self.cancel
            .check()
            .map_err(|e| Failure::new("cancelled", e))?;
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(|| Failure::new("budget", "CPU measurement budget exhausted"))?;
        Ok(())
    }

    fn geometry(&mut self, geometry: &Geometry) -> Result<Value, Failure> {
        if let Some(value) = self.geometries.get(&geometry.id) {
            return Ok(value["bounds"].clone());
        }
        let mut bounds = None;
        let mut contours = vec![];
        let source = match &geometry.data {
            GeometryData::Instances(collection) => {
                self.geometry(&collection.prototype)?;
                let mut bounds = None;
                self.vertices(geometry, &mut |point| include3(&mut bounds, point))?;
                let mut instances = vec![];
                for instance in &collection.instances {
                    self.step()?;
                    instances.push(json!({"id":instance.id,"source_point":instance.source_point,"position":instance.position,"scale":instance.scale,"rotation_z":instance.rotation_z,"orient":instance.orient,"color":instance.color}));
                }
                let bounds = json!(bounds);
                self.geometries.insert(geometry.id, json!({"id":geometry.id,"unit":geometry.unit,"space":geometry.space,"prototype":collection.prototype.id,"attributes":geometry.attributes,"distribution":collection.distribution.id,"instances":instances,"bounds":bounds,"bounds_kind":"mesh"}));
                return Ok(bounds);
            }
            GeometryData::Solids(solids) => return self.solids(geometry, solids),
            GeometryData::Mesh(mesh) => {
                if let Some(source) = &mesh.source {
                    self.geometry(source)?;
                }
                let mut bounds = None;
                for point in &mesh.prepared.mesh.positions {
                    self.step()?;
                    include3(&mut bounds, point.map(f64::from))?;
                }
                for _ in &mesh.face_ids {
                    self.step()?;
                }
                self.points += mesh.prepared.mesh.positions.len();
                let bounds = json!(bounds);
                self.geometries.insert(geometry.id, json!({"id":geometry.id,"unit":geometry.unit,"space":geometry.space,"bounds":bounds,"bounds_kind":"mesh","mesh":{"vertex_ids":mesh.vertex_ids,"face_ids":mesh.face_ids,"positions":mesh.prepared.mesh.positions,"normals":mesh.prepared.mesh.normals,"indices":mesh.prepared.mesh.indices,"uv":mesh.prepared.uv,"regions":mesh.prepared.regions,"source_vertices":mesh.prepared.source_vertices,"source_geometry":mesh.prepared.source_geometry},"attributes":geometry.attributes,"precision":"f32"}));
                return Ok(bounds);
            }
            GeometryData::Contours(contours) => contours,
            GeometryData::Outlines(outlines) => {
                let text = self.text(&outlines.source)?;
                let bounds = json!(self.outline_bounds(outlines, [0.; 2])?);
                let mut glyphs = vec![];
                for (glyph, topology) in outlines.curves.glyphs.iter().zip(&outlines.topology) {
                    self.step()?;
                    let mut commands = vec![];
                    for (id, command) in topology.command_ids.iter().zip(glyph.path.iter()) {
                        self.step()?;
                        commands.push(json!({"id":id,"command":command}));
                    }
                    glyphs.push(json!({"source_glyph":topology.source_glyph,"glyph_index":glyph.glyph,"contour_ids":topology.contour_ids,"commands":commands,"transform":glyph.transform}));
                }
                self.geometries.insert(geometry.id, json!({"id":geometry.id,"unit":geometry.unit,"space":geometry.space,"source_text":text,"outlined_glyphs":glyphs,"bounds":bounds,"bounds_kind":"control_hull"}));
                return Ok(bounds);
            }
            GeometryData::Text(text) => {
                let value = self.text(text)?;
                let bounds = value["bounds"].clone();
                self.geometries.insert(geometry.id, json!({"id":geometry.id,"unit":geometry.unit,"space":geometry.space,"text":value,"bounds":bounds,"bounds_kind":text.bounds_kind()}));
                return Ok(bounds);
            }
        };
        for contour in source.iter() {
            self.step()?;
            let mut local = None;
            let mut ids = Vec::with_capacity(contour.points.len());
            for point in &contour.points {
                self.step()?;
                include(&mut local, point.position)?;
                include(&mut bounds, point.position)?;
                ids.push(point.id);
                self.points += 1;
            }
            contours.push(
                json!({"id":contour.id,"closed":contour.closed,"point_ids":ids,"bounds":local}),
            );
        }
        let bounds = json!(bounds);
        self.geometries.insert(geometry.id, json!({"id":geometry.id,"unit":geometry.unit,"space":geometry.space,"contours":contours,"bounds":bounds}));
        Ok(bounds)
    }
    fn placement(&mut self, placement: &Placement) -> Result<Value, Failure> {
        self.cancel
            .check()
            .map_err(|e| Failure::new("cancelled", e))?;
        self.geometry(&placement.geometry)?;
        let position = match &placement.transform {
            evaluation::placement::Transform::Screen { position } => *position,
            evaluation::placement::Transform::CameraPlane(plane) => plane.position,
            evaluation::placement::Transform::World(transform) => {
                return self.world_placement(placement, transform);
            }
        };
        let mut bounds = None;
        match &placement.geometry.data {
            GeometryData::Solids(_) | GeometryData::Mesh(_) | GeometryData::Instances(_) => {
                return Err(Failure::invalid(
                    "Screen placement cannot measure world solids".into(),
                ));
            }
            GeometryData::Contours(contours) => {
                for contour in contours.iter() {
                    for point in &contour.points {
                        self.step()?;
                        include(
                            &mut bounds,
                            [
                                point.position[0] + position[0],
                                point.position[1] + position[1],
                            ],
                        )?;
                    }
                }
            }
            GeometryData::Outlines(outlines) => {
                bounds = self.outline_bounds(outlines, position)?;
            }
            GeometryData::Text(text) => {
                self.step()?;
                for _ in &text.offsets {
                    self.step()?;
                }
                for corner in text.layout_bounds() {
                    include(
                        &mut bounds,
                        [position[0] + corner[0], position[1] + corner[1]],
                    )?;
                }
            }
        }
        self.step()?;
        self.placements += 1;
        let mut result = json!({"id":placement.id,"geometry":placement.geometry.id,"mode":"screen","position":position,"opacity":placement.opacity,"bounds":bounds,"bounds_kind":bounds_kind(&placement.geometry),"appearance":placement.geometry.appearance});
        if let evaluation::placement::Transform::CameraPlane(plane) = &placement.transform {
            result["mode"] = json!("camera_plane");
            result["attachment"] = json!(plane);
            result["bounds_space"] = json!("canvas_before_projection");
        }
        Ok(result)
    }
    fn world_placement(
        &mut self,
        placement: &Placement,
        transform: &evaluation::placement::World,
    ) -> Result<Value, Failure> {
        let mut bounds = None;
        self.vertices(&placement.geometry, &mut |point| {
            include3(&mut bounds, transform.point(point))
        })?;
        self.placements += 1;
        Ok(
            json!({"id":placement.id,"geometry":placement.geometry.id,"mode":"world","position":transform.position,"rotation":transform.rotation,"scale":transform.scale,"opacity":placement.opacity,"bounds":bounds,"bounds_kind":"mesh","appearance":placement.geometry.appearance}),
        )
    }
    fn vertices(
        &mut self,
        geometry: &Geometry,
        visit: &mut dyn FnMut([f64; 3]) -> Result<(), Failure>,
    ) -> Result<(), Failure> {
        match &geometry.data {
            GeometryData::Mesh(mesh) => {
                for point in &mesh.prepared.mesh.positions {
                    self.step()?;
                    visit(point.map(f64::from))?;
                }
            }
            GeometryData::Solids(solids) => {
                for glyph in &solids.prepared.glyphs {
                    self.step()?;
                    if let Some(solid) = &glyph.solid {
                        for point in &solid.mesh.positions {
                            self.step()?;
                            visit(solid_point(glyph.transform, point.map(f64::from)))?;
                        }
                    }
                }
            }
            GeometryData::Instances(collection) => {
                for instance in &collection.instances {
                    self.step()?;
                    self.vertices(&collection.prototype, &mut |point| {
                        visit(instance.point(point))
                    })?;
                }
            }
            _ => {
                return Err(Failure::invalid(
                    "World placement requires solid geometry".into(),
                ));
            }
        }
        Ok(())
    }
    fn solids(
        &mut self,
        geometry: &Geometry,
        solids: &evaluation::extrude::TextSolids,
    ) -> Result<Value, Failure> {
        let GeometryData::Outlines(outlines) = &solids.source.data else {
            return Err(Failure::invalid("Missing outline provenance".into()));
        };
        let text = self.text(&outlines.source)?;
        let mut bounds: Option<[[f64; 3]; 2]> = None;
        let mut meshes = BTreeMap::new();
        let mut glyphs = vec![];
        for (glyph, topology) in solids.prepared.glyphs.iter().zip(&solids.glyphs) {
            self.step()?;
            if let (Some(solid), Some(topology)) = (&glyph.solid, &topology.topology) {
                for point in &solid.mesh.positions {
                    self.step()?;
                    let p = solid_point(glyph.transform, point.map(f64::from));
                    include3(&mut bounds, p)?;
                }
                if let std::collections::btree_map::Entry::Vacant(entry) = meshes.entry(topology.id)
                {
                    for _ in topology.vertex_ids.iter().chain(&topology.face_ids) {
                        self.step()?;
                    }
                    self.points += solid.mesh.positions.len();
                    entry.insert(json!({"id":topology.id,"vertex_ids":topology.vertex_ids,"face_ids":topology.face_ids,"positions":solid.mesh.positions,"normals":solid.mesh.normals,"indices":solid.mesh.indices,"source_vertices":solid.source_vertices,"regions":{"front":solid.regions.front,"back":solid.regions.back,"sides":solid.regions.sides}}));
                }
            }
            glyphs.push(json!({"id":topology.id,"source_glyph":topology.source_glyph,"mesh":topology.topology.as_ref().map(|t|t.id),"transform":glyph.transform}));
        }
        let bounds = json!(bounds);
        self.geometries.insert(geometry.id,json!({"id":geometry.id,"space":geometry.space,"unit":geometry.unit,"bounds":bounds,"bounds_kind":"mesh","mesh_precision":"f32","source_text":text,"meshes":meshes,"glyphs":glyphs,"meters_per_canvas":solids.prepared.meters_per_canvas,"depth_meters":solids.prepared.depth_meters,"segments":solids.prepared.segments}));
        Ok(bounds)
    }
    fn outline_bounds(
        &mut self,
        outlines: &evaluation::outlines::TextOutlines,
        position: [f64; 2],
    ) -> Result<Option<[[f64; 2]; 2]>, Failure> {
        use fold_render::vector::Segment;
        let mut bounds = None;
        for glyph in &outlines.curves.glyphs {
            self.step()?;
            let [a, b, c, d, e, f] = glyph.transform;
            for command in glyph.path.iter() {
                self.step()?;
                let points = match command {
                    Segment::Move(p) | Segment::Line(p) => [Some(p), None, None],
                    Segment::Quad(c, p) => [Some(c), Some(p), None],
                    Segment::Cubic(a, b, p) => [Some(a), Some(b), Some(p)],
                    Segment::Close => [None; 3],
                };
                for p in points.into_iter().flatten() {
                    include(
                        &mut bounds,
                        [
                            a * p[0] + c * p[1] + e + position[0],
                            b * p[0] + d * p[1] + f + position[1],
                        ],
                    )?;
                }
            }
        }
        Ok(bounds)
    }
    fn text(&mut self, text: &evaluation::text::Text) -> Result<Value, Failure> {
        self.step()?;
        let layout = &text.layout;
        let hash = hex(&layout.font_hash);
        let font = json!({"resource":text.resource,"family":evaluation::text::FONT_FAMILY,"fingerprint":format!("sha256:{hash}"),"source":"packaged"});
        self.fonts.insert(hash, font.clone());
        let mut glyphs = vec![];
        for ((id, glyph), offset) in text.glyph_ids.iter().zip(&layout.glyphs).zip(&text.offsets) {
            self.step()?;
            glyphs.push(
                json!({"id":id,"glyph":glyph.id,"position":[f64::from(glyph.position[0])+offset[0],f64::from(glyph.position[1])+offset[1]],"layout_position":glyph.position,"offset":offset,"advance":glyph.advance}),
            );
            self.glyphs += 1;
        }
        let mut clusters = vec![];
        for (id, cluster) in text.cluster_ids.iter().zip(&layout.clusters) {
            self.step()?;
            clusters.push(json!({"id":id,"text":cluster.text,"glyphs":cluster.glyphs,"advance":cluster.advance,"word_boundary":cluster.word_boundary,"ligature_continuation":cluster.ligature_continuation}));
        }
        let mut runs = vec![];
        let mut words = vec![];
        for (id, word) in text.word_ids.iter().zip(&layout.words) {
            self.step()?;
            let mut cluster_ids = vec![];
            let mut glyph_ids = vec![];
            for &index in &word.clusters {
                self.step()?;
                cluster_ids.push(text.cluster_ids[index]);
            }
            for &index in &word.glyphs {
                self.step()?;
                glyph_ids.push(text.glyph_ids[index]);
            }
            let offset = word
                .glyphs
                .first()
                .map_or([0.; 2], |&glyph| text.offsets[glyph]);
            let pivot = [0, 1].map(|axis| f64::from(word.pivot[axis]) + offset[axis]);
            words.push(json!({"id":id,"text":word.text,"clusters":cluster_ids,"glyphs":glyph_ids,"pivot":pivot,"layout_pivot":word.pivot,"offset":offset,"pivot_kind":"advance_center"}));
        }
        for run in &layout.runs {
            self.step()?;
            runs.push(json!({"line":run.line,"text":run.text,"clusters":run.clusters,"glyphs":run.glyphs,"face_index":run.face_index,"font_size":run.font_size,"normalized_coords":run.normalized_coords,"rtl":run.rtl,"baseline":run.baseline}));
        }
        let mut diagnostics = vec![];
        for diagnostic in &layout.diagnostics {
            self.step()?;
            match diagnostic {
                fold_render::text::Diagnostic::MissingGlyph { text } => {
                    diagnostics.push(json!({"kind":"missing_glyph","text":text}))
                }
            }
        }
        for _ in &text.line_ids {
            self.step()?;
        }
        Ok(
            json!({"text":layout.text.as_ref(),"identity":hex(&layout.identity),"font":font,"line_ids":text.line_ids,"line_count":layout.line_count,"glyphs":glyphs,"clusters":clusters,"words":words,"runs":runs,"diagnostics":diagnostics,"bounds":text.layout_bounds(),"bounds_kind":text.bounds_kind(),"shaping_precision":"f32"}),
        )
    }
    fn value(&mut self, value: &evaluation::Value) -> Result<Value, Failure> {
        match value {
            evaluation::Value::Primitive(value) => Ok(json!({"kind":"primitive","value":value})),
            evaluation::Value::Geometry(geometry) => Ok(
                json!({"kind":"geometry","id":geometry.id,"bounds":self.geometry(geometry)?,"bounds_kind":bounds_kind(geometry),"appearance":geometry.appearance,"attributes":geometry.attributes}),
            ),
            evaluation::Value::Placement(placement) => {
                Ok(json!({"kind":"placement","placement":self.placement(placement)?}))
            }
            evaluation::Value::Camera(camera) => {
                self.step()?;
                Ok(json!({"kind":"camera","camera":camera.as_ref()}))
            }
            evaluation::Value::Light(light) => {
                self.step()?;
                Ok(json!({"kind":"light","light":light.as_ref()}))
            }
            evaluation::Value::Scene(scene) => {
                if scene.camera.is_some() {
                    self.step()?;
                }
                let mut entries = Vec::with_capacity(scene.entries.len());
                for entry in &scene.entries {
                    self.step()?;
                    entries
                        .push(json!({"id":entry.id,"placement":self.placement(&entry.placement)?}));
                }
                let mut cameras = vec![];
                for entry in &scene.cameras {
                    self.step()?;
                    cameras.push(json!({"id":entry.id,"camera":entry.camera.as_ref()}));
                }
                let mut lights = vec![];
                for entry in &scene.lights {
                    self.step()?;
                    lights.push(json!({"id":entry.id,"light":entry.light.as_ref()}));
                }
                Ok(
                    json!({"kind":"scene","entries":entries,"camera":scene.camera.as_deref(),"cameras":cameras,"lights":lights}),
                )
            }
        }
    }
}
fn bounds_kind(geometry: &Geometry) -> &'static str {
    match &geometry.data {
        GeometryData::Contours(_) => "geometry",
        GeometryData::Text(text) => text.bounds_kind(),
        GeometryData::Outlines(_) => "control_hull",
        GeometryData::Solids(_) | GeometryData::Mesh(_) | GeometryData::Instances(_) => "mesh",
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn include(bounds: &mut Option<[[f64; 2]; 2]>, point: [f64; 2]) -> Result<(), Failure> {
    if !point.iter().all(|x| x.is_finite()) {
        return Err(Failure::invalid("Non-finite evaluated bounds".into()));
    }
    match bounds {
        None => *bounds = Some([point, point]),
        Some([min, max]) => {
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_and_measurement_exhaustion_are_explicit() {
        let id = fold_foundation::DocumentId::new();
        let document = fold_motion::package::create(id).unwrap();
        let construction = Construction::from_document(&document).unwrap();
        let cancel = Cancel::default();
        cancel.cancel();
        let error = run(
            ObjectId::new(),
            &document,
            Revision(0),
            None,
            construction.definitions[0].outputs[0].id,
            &[Time::ZERO],
            &cancel,
        )
        .expect_err("cancelled observation");
        assert_eq!(error.code, "cancelled");
        let cancel = Cancel::default();
        let mut measurement = Measurement {
            cancel: &cancel,
            geometries: BTreeMap::new(),
            points: 0,
            placements: 0,
            glyphs: 0,
            fonts: BTreeMap::new(),
            remaining: 0,
        };
        let geometry = Geometry {
            attributes: Default::default(),
            id: 1,
            space: fold_motion::types::Space::Screen,
            unit: fold_motion::types::Unit::Canvas,
            data: GeometryData::Contours(std::sync::Arc::new(vec![evaluation::Contour {
                id: 2,
                closed: false,
                points: vec![evaluation::Point {
                    id: 3,
                    position: [0., 0.],
                }],
            }])),
            appearance: Default::default(),
        };
        assert_eq!(
            measurement.geometry(&geometry).err().unwrap().code,
            "budget"
        );
        assert!(measurement.geometries.is_empty());
    }
}

fn solid_point(transform: [f64; 6], point: [f64; 3]) -> [f64; 3] {
    let [a, b, c, d, e, f] = transform;
    let [x, y, z] = point;
    [a * x + c * y + e, b * x + d * y + f, z]
}
fn include3(bounds: &mut Option<[[f64; 3]; 2]>, p: [f64; 3]) -> Result<(), Failure> {
    if !p.iter().all(|v| v.is_finite()) {
        return Err(Failure::invalid("Non-finite solid bounds".into()));
    }
    match bounds {
        None => *bounds = Some([p, p]),
        Some([min, max]) => {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
    }
    Ok(())
}
