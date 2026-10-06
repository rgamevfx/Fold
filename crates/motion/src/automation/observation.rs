//! Evaluated evidence, deliberately distinct from authored graph inspection.
use crate::{
    Motion,
    evaluation::{Evaluator, Value as Evaluated},
    fields::{Budget, Sample},
    geometry::{Content, Geometry, IDENTITY, multiply},
};
use fold_foundation::{ObjectId, Time};
use fold_media::Cancel;
use serde::Deserialize;
use serde_json::{Value, json};
use std::hash::{Hash, Hasher};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Query {
    mode: String,
    time: Time,
    #[serde(default)]
    object: Option<ObjectId>,
    #[serde(default)]
    node: Option<ObjectId>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    fields: Option<Vec<String>>,
    #[serde(default)]
    limit: Option<usize>,
}
pub fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Query)).expect("observation schema")
}
pub fn evaluate(m: &Motion, q: &Value, cancel: &Cancel) -> Result<Value, String> {
    let q: Query = fold_platform::authoring::decode(q)?;
    if q.mode != "evaluated" {
        return Err("expected evaluated mode".into());
    }
    if q.object.is_some() && q.node.is_some() {
        return Err("select object or node, not both".into());
    }
    if q.output.is_some() && q.node.is_none() {
        return Err("output requires node".into());
    }
    let limit = q.limit.unwrap_or(32);
    if !(1..=200).contains(&limit) {
        return Err("limit must be 1..200".into());
    }
    let fields = q
        .fields
        .unwrap_or_else(|| vec!["counts".into(), "elements".into()]);
    for field in &fields {
        if !["counts", "elements", "bounds"].contains(&field.as_str()) {
            return Err(format!(
                "unsupported evaluated field {field}; supported: counts, elements, bounds"
            ));
        }
    }
    let mut evaluator = Evaluator::new(m, q.time)?;
    evaluator.budget = Budget::cancellable(cancel);
    let value = if let Some(id) = q.object {
        Evaluated::Content(evaluator.world_content(id)?)
    } else if let Some(id) = q.node {
        evaluator.resolve(&crate::graph::Link {
            node: id,
            socket: q.output.ok_or("node requires output socket")?,
        })?
    } else {
        evaluator.scene()?
    };
    let mut result = json!({"time":q.time,"space":if q.node.is_some(){"node output coordinates; depends on upstream construction"}else{"document"},"target":{"object":q.object,"node":q.node},"evidence":"evaluated geometry; visibility includes effects, masks and clipping only in rendered images","unsupported":["paint_bounds","layout_bounds","parameter_contributions","pixel_visibility","provenance"]});
    match value {
        Evaluated::Field(field) => {
            result["kind"] = json!(field.kind);
            if field.varying {
                result["status"] = json!("inconclusive");
                result["reason"] = json!("field requires a consuming element domain");
            } else {
                result["value"] = json!(field.sample(&Sample::default(), &mut evaluator.budget)?);
            }
        }
        Evaluated::Content(c) | Evaluated::Points(c) => {
            if fields.iter().any(|f| f == "bounds") {
                result["bounds"] = json!(crate::geometry::bounds::content(&c, cancel)?);
                result["bounds_kind"] = json!(
                    "analytic outline/point bounds in the reported coordinate space; excludes stroke, effects, masks and clipping"
                );
            }
            let mut counts = [0usize; 5];
            let mut elements = Vec::new();
            walk(
                &c,
                IDENTITY,
                1.,
                0,
                &mut counts,
                &mut elements,
                limit,
                cancel,
            )?;
            if fields.iter().any(|s| s == "counts") {
                result["counts"] = json!({"top_level":c.len(),"paths":counts[0],"glyphs":counts[1],"points":counts[2],"instances":counts[3],"groups":counts[4]});
            }
            if fields.iter().any(|s| s == "elements") {
                result["elements"] = json!(elements);
                result["truncated"] = json!(counts.iter().sum::<usize>() > limit);
            }
            if fields.iter().any(|s| s == "counts") {
                result["top_level_domains"] = json!({"children":c.len(),"glyphs":c.iter().filter(|e|e.matches(crate::geometry::Domain::Glyphs)).count(),"points":c.iter().filter(|e|e.matches(crate::geometry::Domain::Points)).count(),"instances":c.iter().filter(|e|e.matches(crate::geometry::Domain::Instances)).count()});
                if counts[1] > 0 && !c.iter().any(|e| e.matches(crate::geometry::Domain::Glyphs)) {
                    result["diagnostics"] = json!([{"kind":"nested_glyphs","message":"Glyphs exist only inside groups/instances; a top-level Glyphs consumer matches zero elements. Whole-object motion can use the object's transform; descendant glyph editing needs explicit traversal."}]);
                }
            }
            result["empty"] = json!(counts[0] + counts[1] + counts[2] == 0);
        }
    }
    Ok(result)
}
#[allow(clippy::too_many_arguments)]
fn walk(
    c: &Content,
    parent: [f64; 6],
    opacity: f64,
    depth: usize,
    counts: &mut [usize; 5],
    elements: &mut Vec<Value>,
    limit: usize,
    cancel: &Cancel,
) -> Result<(), String> {
    if depth > 64 {
        return Err("observation nesting exceeds 64".into());
    }
    for e in c.iter() {
        cancel.check()?;
        if counts.iter().sum::<usize>() >= 100_000 {
            return Err("observation exceeds 100000 elements".into());
        }
        let transform = multiply(parent, e.transform);
        let opacity = opacity * e.opacity;
        let (index, kind) = match &e.geometry {
            Geometry::Path(_) => (0, "path"),
            Geometry::Glyph { .. } => (1, "glyph"),
            Geometry::Point => (2, "point"),
            Geometry::Instance(_) => (3, "instance"),
            Geometry::Group(_) => (4, "group"),
        };
        counts[index] += 1;
        if elements.len() < limit {
            let mut value = json!({"id":e.id,"kind":kind,"depth":depth,"transform":transform,"opacity":opacity});
            if let Geometry::Path(path) | Geometry::Glyph { path, .. } = &e.geometry {
                let mut signature = std::collections::hash_map::DefaultHasher::new();
                for segment in path.iter() {
                    cancel.check()?;
                    format!("{segment:?}").hash(&mut signature);
                }
                value["path_signature"] =
                    json!(format!("diagnostic64:{:016x}", signature.finish()));
                value["path_segments"] = json!(path.len());
                value["subpaths"] = json!(
                    path.iter()
                        .filter(|s| matches!(s, fold_render::vector::Segment::Move(_)))
                        .count()
                );
            }
            value["fill"] = json!(e.fill);
            value["stroke"] = json!(e.stroke);
            value["image_effects"] = json!(e.effects.is_some());
            if let Geometry::Glyph { cluster, .. } = &e.geometry {
                value["cluster"] = json!(cluster);
                value["advance"] = json!(e.attributes.get("fold.text.advance"));
                value["glyph_id"] = json!(e.attributes.get("fold.text.glyph_id"));
            }
            elements.push(value);
        }
        if let Geometry::Group(children) | Geometry::Instance(children) = &e.geometry {
            walk(
                children,
                transform,
                opacity,
                depth + 1,
                counts,
                elements,
                limit,
                cancel,
            )?;
        }
    }
    Ok(())
}

/// Authored inputs and native evaluated contributions from one exact-time evaluator.
pub fn parameters(m: &Motion, query: &Value, cancel: &Cancel) -> Result<Value, String> {
    let id: ObjectId = serde_json::from_value(query["node"].clone()).map_err(|e| e.to_string())?;
    let time: Time = serde_json::from_value(query["time"].clone()).map_err(|e| e.to_string())?;
    let node = m.graph.node(id)?;
    let (inputs, _) = m.signature(node)?;
    let mut evaluator = Evaluator::new(m, time)?;
    evaluator.budget = Budget::cancellable(cancel);
    let mut values = vec![];
    for socket in inputs {
        cancel.check()?;
        let evaluated = match evaluator.input(node, &socket.id) {
            Ok(Evaluated::Field(field)) if !field.varying => {
                match field.uniform(&mut evaluator.budget) {
                    Ok(value) => json!({"status":"evaluated","value":value}),
                    Err(error) => json!({"status":"unavailable","reason":error}),
                }
            }
            Ok(Evaluated::Field(_)) => {
                json!({"status":"inconclusive","reason":"requires a consuming element domain"})
            }
            Ok(Evaluated::Content(content) | Evaluated::Points(content)) => {
                let domain = serde_json::from_value::<crate::geometry::Domain>(
                    node.settings["domain"].clone(),
                )
                .ok();
                let matched = domain.map(|domain| {
                    content
                        .iter()
                        .filter(|element| element.matches(domain))
                        .count()
                });
                json!({"status":"geometry","top_level_count":content.len(),"domain":domain,"matched_count":matched,"diagnostic":if matched == Some(0) { Some("selected domain matches zero top-level elements; inspect descendants and choose an explicit scope") } else { None },"guidance":"inspect evaluated output for counts and bounds"})
            }
            Err(error) => json!({"status":"unavailable","reason":error}),
        };
        values.push(json!({"input":socket.id,"unit":socket.unit,"kind":socket.kind,"binding":node.inputs.get(&socket.id),"default":socket.default,"animation":node.animation.get(&socket.id),"evaluated":evaluated}));
    }
    Ok(
        json!({"node":id,"time":time,"parameters":values,"composition":"native evaluator resolves the authored binding and parameter animation; varying fields require an explicit consumer domain"}),
    )
}

pub fn text_metrics(m: &Motion, query: &Value, cancel: &Cancel) -> Result<Value, String> {
    let id: ObjectId = serde_json::from_value(query["node"].clone()).map_err(|e| e.to_string())?;
    let time: Time = serde_json::from_value(query["time"].clone()).map_err(|e| e.to_string())?;
    let node = m.graph.node(id)?;
    if node.kind != "fold.motion.text" {
        return Err("text_metrics requires a Text source node; use evaluated bounds for downstream transformed geometry".into());
    }
    let settings = node.settings::<crate::nodes::content::text::Settings>()?;
    let font_id = settings.font.ok_or("Text requires an embedded font")?;
    let font = m.fonts.get(&font_id).ok_or("missing embedded font")?;
    let mut evaluator = Evaluator::new(m, time)?;
    evaluator.budget = Budget::cancellable(cancel);
    let text = evaluator.uniform(node, "text")?.text()?.to_owned();
    let size = evaluator.scalar(node, "size")?;
    let spacing = evaluator.scalar(node, "line_spacing")?;
    let alignment = evaluator.scalar(node, "alignment")?;
    cancel.check()?;
    let layout =
        crate::geometry::text::shape_layout(&text, font, size, spacing, alignment, node.seed())?;
    let ink = crate::geometry::bounds::content_with_budget(&layout.content, &mut evaluator.budget)?;
    let mut bounds = crate::geometry::bounds::Bounds::default();
    for line in &layout.lines {
        bounds.point([line.bounds[0], line.bounds[1]]);
        bounds.point([line.bounds[2], line.bounds[3]]);
    }
    let limit = query["limit"].as_u64().unwrap_or(32) as usize;
    let glyphs: Vec<_> = layout.content.iter().take(limit).map(|glyph| {
        let Geometry::Glyph { cluster, .. } = glyph.geometry else { unreachable!() };
        json!({"id":glyph.id,"cluster":cluster,"position":[glyph.transform[4],glyph.transform[5]],"advance":glyph.attributes.get("fold.text.advance"),"glyph_id":glyph.attributes.get("fold.text.glyph_id")})
    }).collect();
    Ok(
        json!({"node":id,"time":time,"space":"text source local coordinates, before object transforms/modifiers","font":{"id":font_id,"name":font.name,"face_index":font.face_index,"fingerprint":super::font_identity(font)},"text":text,"size":size,"line_spacing":spacing,"alignment":alignment,"ink_bounds":ink,"layout_bounds":bounds.0,"layout_definition":"per-line advance and embedded-font ascender/descender; empty lines retain vertical metrics","line_count":layout.lines.len(),"lines":layout.lines.iter().take(limit).collect::<Vec<_>>(),"glyph_count":layout.content.len(),"glyphs":glyphs,"truncated":layout.content.len()>limit || layout.lines.len()>limit,"identity_contract":"glyph IDs derive from source node, shaped cluster and run ordinal; text edits may change correspondence"}),
    )
}
