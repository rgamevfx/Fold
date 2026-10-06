//! Bounded read-only installed-font discovery. Called on the inspection worker.
use fold_media::Cancel;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};

pub fn discover(query: &Value, cancel: &Cancel) -> Result<Value, String> {
    if query
        .get("query")
        .is_some_and(|q| q.as_str().is_none_or(|q| q.len() > 256))
    {
        return Err("font query must be a string of at most 256 bytes".into());
    }
    let mut roots = vec![];
    #[cfg(target_os = "linux")]
    {
        roots.extend([
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
        ]);
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(&home).join(".local/share/fonts"));
            roots.push(PathBuf::from(home).join(".fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        roots.extend([
            PathBuf::from("/System/Library/Fonts"),
            PathBuf::from("/Library/Fonts"),
        ]);
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(root) = std::env::var_os("WINDIR") {
            roots.push(PathBuf::from(root).join("Fonts"));
        }
        if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(root).join("Microsoft/Windows/Fonts"));
        }
    }
    scan(roots, query, cancel)
}
fn scan(roots: Vec<PathBuf>, query: &Value, cancel: &Cancel) -> Result<Value, String> {
    cancel.check()?;
    let mut pending = roots.clone();
    let mut visited = BTreeSet::new();
    let mut files = BTreeSet::new();
    let mut truncated = false;
    let mut unreadable = 0;
    while let Some(directory) = pending.pop() {
        cancel.check()?;
        if visited.len() >= 128 || files.len() >= 2048 {
            truncated = true;
            break;
        }
        let Ok(directory) = directory.canonicalize() else {
            continue;
        };
        if !visited.insert(directory.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            unreadable += 1;
            continue;
        };
        let mut entries = entries
            .take(4097)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if entries.len() > 4096 {
            entries.truncate(4096);
            truncated = true;
        }
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            cancel.check()?;
            let path = entry.path();
            if path.is_dir() {
                if pending.len() < 128 {
                    pending.push(path);
                } else {
                    truncated = true;
                }
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                ["ttf", "otf", "ttc", "otc"].contains(&e.to_ascii_lowercase().as_str())
            }) {
                if files.len() >= 2048 {
                    truncated = true;
                    break;
                }
                files.insert(path);
            }
        }
    }
    let mut bytes = 0u64;
    let mut fonts = vec![];
    for path in files {
        cancel.check()?;
        let Ok(size) = path.metadata().map(|m| m.len()) else {
            unreadable += 1;
            continue;
        };
        if size > 8 * 1024 * 1024 {
            unreadable += 1;
            continue;
        }
        if bytes + size > 64 * 1024 * 1024 {
            truncated = true;
            break;
        }
        bytes += size;
        // Read at most the import budget even if a file grows after metadata.
        use std::io::Read;
        let data = std::fs::File::open(&path).and_then(|file| {
            let mut data = vec![];
            file.take(8 * 1024 * 1024 + 1).read_to_end(&mut data)?;
            Ok(data)
        });
        let Ok(data) = data else {
            unreadable += 1;
            continue;
        };
        if data.len() > 8 * 1024 * 1024 {
            unreadable += 1;
            continue;
        }
        let count = rustybuzz::ttf_parser::fonts_in_collection(&data).unwrap_or(1);
        truncated |= count > 16;
        for index in 0..count.min(16) {
            cancel.check()?;
            let Some(face) = rustybuzz::Face::from_slice(&data, index) else {
                unreadable += 1;
                continue;
            };
            let name = |ids: &[u16]| {
                ids.iter().find_map(|id| {
                    face.names()
                        .into_iter()
                        .find_map(|n| (n.name_id == *id).then(|| n.to_string()).flatten())
                })
            };
            fonts.push(json!({"path":path,"face_index":index,"family":name(&[16,1]),"style":name(&[17,2]),"weight":face.weight().to_number(),"italic":face.is_italic(),"bytes":size}));
        }
    }
    let search = query["query"].as_str().unwrap_or("").to_lowercase();
    fonts.retain(|font| font.to_string().to_lowercase().contains(&search));
    fonts.sort_by_key(|font| {
        (
            font["family"].to_string(),
            font["weight"].as_u64(),
            font["style"].to_string(),
            font["path"].to_string(),
            font["face_index"].as_u64(),
        )
    });
    let total = fonts.len();
    let offset = query["offset"].as_u64().unwrap_or(0).min(usize::MAX as u64) as usize;
    let limit = query["limit"].as_u64().unwrap_or(50) as usize;
    Ok(
        json!({"fonts":fonts.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"total":total,"offset":offset,"limit":limit,"scan_truncated":truncated,"unreadable_or_unsupported":unreadable,"searched_roots":roots,"import":"Use import_font with returned absolute path and face_index, then bind Text settings.font to the returned font ID. Import embeds bytes so saved artwork does not depend on installed fonts.","limits":{"directories":128,"files":2048,"file_bytes":8388608,"scanned_bytes":67108864,"faces_per_file":16}}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn font_discovery_reports_importable_faces_and_honors_cancellation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.ttf");
        std::fs::write(&path, crate::document::Font::builtin().bytes()).unwrap();
        let result = scan(
            vec![directory.path().to_owned()],
            &json!({"query":"noto","limit":1}),
            &Cancel::default(),
        )
        .unwrap();
        assert_eq!(result["total"], 1);
        assert_eq!(result["fonts"][0]["family"], "Noto Sans");
        assert_eq!(result["fonts"][0]["weight"], 400);
        assert_eq!(result["fonts"][0]["face_index"], 0);
        assert_eq!(result["fonts"][0]["path"], json!(path));
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(scan(vec![], &json!({}), &cancel).is_err());
    }
}
