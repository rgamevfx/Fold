//! Portable native assets. Stable identities make repeated imports idempotent;
//! conflicting definitions are rejected instead of overwriting artist work.
use super::*;
use crate::document::Font;

#[derive(Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub format: u32,
    pub entry: ObjectId,
    pub groups: BTreeMap<ObjectId, GroupDefinition>,
    pub fonts: BTreeMap<ObjectId, Font>,
    pub programs: BTreeMap<ObjectId, Record>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub fn export(m: &Motion, entry: ObjectId) -> Result<Bundle, String> {
    let mut bundle = Bundle {
        format: 1,
        entry,
        groups: BTreeMap::new(),
        fonts: BTreeMap::new(),
        programs: BTreeMap::new(),
        extensions: m
            .extensions
            .get("fold.motion.asset_bundle_extensions")
            .and_then(|v| v.get(record_key(entry)))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
    };
    let mut pending = vec![entry];
    while let Some(id) = pending.pop() {
        if bundle.groups.contains_key(&id) {
            continue;
        }
        let group = m.groups.get(&id).ok_or("missing asset dependency")?.clone();
        for n in &group.graph.nodes {
            // Scene/media references must become declared inputs to cross projects.
            if n.kind == "fold.motion.object_reference"
                || n.kind == "fold.motion.scene_children"
                || n.kind == "fold.motion.published_control"
            {
                return Err("asset contains a project-owned reference; expose it as an asset input before packaging".into());
            }
            if n.kind == "fold.motion.text" {
                if let Some(v) = n.settings.get("font").filter(|v| !v.is_null()) {
                    let font: ObjectId =
                        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
                    bundle.fonts.insert(
                        font,
                        m.fonts.get(&font).ok_or("missing embedded font")?.clone(),
                    );
                }
            }
            if matches!(
                n.kind.as_str(),
                "fold.motion.group_instance" | "fold.motion.solver"
            ) {
                if let Some(v) = n.settings.get("group").filter(|v| !v.is_null()) {
                    pending.push(serde_json::from_value(v.clone()).map_err(|e| e.to_string())?);
                }
            }
        }
        if let Some(record) = record(m, id)? {
            bundle.programs.insert(id, record);
        }
        bundle.groups.insert(id, group);
        if bundle.groups.len() > 128 {
            return Err("asset dependency budget exceeded".into());
        }
    }
    // Pin all fonts retained in source identity history, including removed nodes.
    for record in bundle.programs.values() {
        for (key, id) in &record.ids {
            if key.starts_with("!font/") {
                bundle.fonts.insert(
                    *id,
                    m.fonts
                        .get(id)
                        .ok_or("source history references missing font")?
                        .clone(),
                );
            }
        }
    }
    validate(&bundle)?;
    Ok(bundle)
}
fn validate(bundle: &Bundle) -> Result<(), String> {
    if bundle.format != 1
        || !bundle.groups.contains_key(&bundle.entry)
        || bundle
            .programs
            .keys()
            .any(|id| !bundle.groups.contains_key(id))
    {
        return Err("invalid asset bundle version or entry".into());
    }
    let mut m = Motion::empty();
    m.groups = bundle.groups.clone();
    m.fonts = bundle.fonts.clone();
    // Validation does not compile/execute source, including on import.
    m.validate()?;
    let group = &m.groups[&bundle.entry];
    let output = group
        .outputs
        .get("content")
        .ok_or("portable asset requires a content output")?;
    if !m
        .signature(group.graph.node(output.node)?)?
        .1
        .iter()
        .any(|(socket, kind)| socket == &output.socket && *kind == Kind::Content)
    {
        return Err("portable asset output must be Content".into());
    }
    for (id, record) in &bundle.programs {
        if record.version != 1 || record.source.len() > 65536 || record.ids.len() > 4096 {
            return Err("invalid bundled source metadata".into());
        }
        let mut baseline = m.clone();
        baseline.groups.insert(*id, record.baseline.clone());
        baseline.validate()?;
    }
    let document = m.document(fold_foundation::DocumentId::new())?;
    if !document.dependencies.is_empty() || !document.assets.is_empty() {
        return Err("asset contains external media/document dependencies; expose these as inputs before packaging".into());
    }
    for g in m.groups.values() {
        for n in &g.graph.nodes {
            if matches!(
                n.kind.as_str(),
                "fold.motion.object_reference"
                    | "fold.motion.scene_children"
                    | "fold.motion.published_control"
            ) {
                return Err("bundle contains a project-owned reference".into());
            }
            if n.kind == "fold.motion.text" {
                let id: ObjectId = serde_json::from_value(
                    n.settings
                        .get("font")
                        .cloned()
                        .ok_or("bundle text requires embedded font")?,
                )
                .map_err(|e| e.to_string())?;
                if !m.fonts.contains_key(&id) {
                    return Err("bundle is missing an embedded font".into());
                }
            }
        }
    }
    Ok(())
}
pub fn import(m: &mut Motion, bundle: &Bundle) -> Result<ObjectId, String> {
    validate(bundle)?;
    let mut candidate = m.clone();
    for (id, font) in &bundle.fonts {
        if candidate.fonts.get(id).is_some_and(|old| old != font) {
            return Err("font identity conflicts with this project".into());
        }
        candidate.fonts.insert(*id, font.clone());
    }
    for (id, group) in &bundle.groups {
        if candidate.groups.get(id).is_some_and(|old| old != group) {
            return Err(
                "asset identity conflicts with this project; existing artwork was retained".into(),
            );
        }
        candidate.groups.insert(*id, group.clone());
    }
    for (id, r) in &bundle.programs {
        let value = serde_json::to_value(r).map_err(|e| e.to_string())?;
        if record(&candidate, *id)?
            .is_some_and(|old| serde_json::to_value(old).ok() != Some(value.clone()))
        {
            return Err("asset source identity conflicts with this project".into());
        }
        let metadata = candidate
            .extensions
            .entry(METADATA.into())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("invalid asset metadata")?;
        metadata.insert(record_key(*id), value);
    }
    if !bundle.extensions.is_empty() {
        candidate
            .extensions
            .entry("fold.motion.asset_bundle_extensions".into())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("invalid bundle extension metadata")?
            .insert(record_key(bundle.entry), json!(bundle.extensions));
    }
    candidate.validate()?;
    *m = candidate;
    Ok(bundle.entry)
}
pub fn read(path: &std::path::Path) -> Result<Bundle, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("asset bundle exceeds 64 MiB".into());
    }
    let bundle: Bundle = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    validate(&bundle)?;
    Ok(bundle)
}
pub fn write(bundle: &Bundle, path: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    validate(bundle)?;
    let bytes = serde_json::to_vec(bundle).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("asset bundle exceeds 64 MiB".into());
    }
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("asset path requires a directory")?)
            .map_err(|e| e.to_string())?;
    temp.write_all(&bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist_noclobber(path).map_err(|e| e.to_string())?;
    Ok(())
}
