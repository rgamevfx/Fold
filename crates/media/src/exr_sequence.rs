//! Verified, relative frame manifests. Discovery and hashing run on ingest workers.
use crate::{
    Cancel,
    exr::{Info, Source},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    pub frames: BTreeMap<u32, Frame>,
}
/// Small per-use contract; verified paths/hashes stay on the imported asset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub first: u32,
    pub last: u32,
    pub count: usize,
    pub numbering_hash: String,
}
impl Summary {
    pub fn validate(&self) -> Result<(), String> {
        if self.first > self.last
            || self.last >= i32::MAX as u32
            || self.count == 0
            || self.count > 18000
            || self.count as u64 > u64::from(self.last - self.first) + 1
            || self.numbering_hash.len() != 64
        {
            return Err("Invalid EXR sequence summary".into());
        }
        Ok(())
    }
    pub fn matches(&self, sequence: &Sequence) -> bool {
        self == &sequence.summary()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub file: String,
    pub fingerprint: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Policy {
    #[default]
    Error,
    Black,
    Hold,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    pub still: bool,
    pub missing: Policy,
    pub outside: Policy,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            still: false,
            missing: Policy::Error,
            outside: Policy::Black,
        }
    }
}
impl Sequence {
    pub fn summary(&self) -> Summary {
        Summary {
            first: self.first(),
            last: self.last(),
            count: self.frames.len(),
            numbering_hash: crate::content_hash(
                &self
                    .frames
                    .keys()
                    .flat_map(|n| n.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
        }
    }

    pub fn first(&self) -> u32 {
        *self.frames.first_key_value().expect("validated sequence").0
    }
    pub fn last(&self) -> u32 {
        *self.frames.last_key_value().expect("validated sequence").0
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.frames.is_empty() || self.frames.len() > 18000 {
            return Err("EXR sequence requires 1–18000 frames".into());
        }
        for (number, frame) in &self.frames {
            if *number >= i32::MAX as u32
                || Path::new(&frame.file).components().count() != 1
                || !matches!(
                    Path::new(&frame.file).components().next(),
                    Some(std::path::Component::Normal(_))
                )
                || !frame.fingerprint.starts_with("sha256:")
            {
                return Err("Invalid EXR sequence frame".into());
            }
        }
        Ok(())
    }
}
fn numbered(name: &str) -> Option<(&str, &str, &str, u32)> {
    let (stem, extension) = name.rsplit_once('.')?;
    if !extension.eq_ignore_ascii_case("exr") {
        return None;
    }
    let start = stem.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let digits = &stem[start..];
    if digits.is_empty() {
        return None;
    }
    Some((&stem[..start], digits, extension, digits.parse().ok()?))
}
pub fn discover(path: &Path, info: &Info, cancel: &Cancel) -> Result<Option<Sequence>, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("EXR path must be UTF-8")?;
    let Some((prefix, _, extension, _)) = numbered(name) else {
        return Ok(None);
    };
    let parent = path.parent().ok_or("EXR has no parent directory")?;
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(parent).map_err(|e| e.to_string())? {
        cancel.check()?;
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some((candidate, _, ext, frame)) = numbered(name) else {
            continue;
        };
        if candidate != prefix || ext != extension {
            continue;
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        if files.insert(frame, name.to_owned()).is_some() {
            return Err("Ambiguous EXR frame numbering".into());
        }
        if files.len() > 18000 {
            return Err("EXR sequence exceeds 18000 frames".into());
        }
    }
    if files.len() < 2 {
        return Ok(None);
    }
    let mut frames = BTreeMap::new();
    for (number, file) in files {
        cancel.check()?;
        let path = parent.join(&file);
        let fingerprint = crate::fingerprint(&path, cancel)?;
        if &crate::exr::inspect(&path, cancel)? != info {
            return Err(format!(
                "EXR sequence frame {number} changes channels or image windows"
            ));
        }
        frames.insert(number, Frame { file, fingerprint });
    }
    let sequence = Sequence { frames };
    sequence.validate()?;
    Ok(Some(sequence))
}
/// `frame` is the exact source frame number. None means transparent black.
pub fn resolve(
    source: &Source,
    frame: i64,
    options: &Options,
    outside: bool,
) -> Result<Option<Source>, String> {
    let mut result = source.clone();
    let sequence = result.info.sequence.take();
    if options.still || sequence.is_none() {
        return Ok((!outside).then_some(result));
    }
    let sequence = sequence.unwrap();
    sequence.validate()?;
    let outside =
        outside || frame < i64::from(sequence.first()) || frame > i64::from(sequence.last());
    let found = u32::try_from(frame)
        .ok()
        .and_then(|n| sequence.frames.get(&n));
    let selected = if outside || found.is_none() {
        match if outside {
            options.outside
        } else {
            options.missing
        } {
            Policy::Error => {
                return Err(format!(
                    "EXR source frame {frame} is {}",
                    if outside {
                        "outside the Read range"
                    } else {
                        "missing"
                    }
                ));
            }
            Policy::Black => return Ok(None),
            Policy::Hold => {
                let number =
                    frame.clamp(i64::from(sequence.first()), i64::from(sequence.last())) as u32;
                sequence
                    .frames
                    .range(..=number)
                    .next_back()
                    .map(|(_, f)| f)
                    .or_else(|| sequence.frames.first_key_value().map(|(_, f)| f))
                    .unwrap()
            }
        }
    } else {
        found.unwrap()
    };
    result.path = result
        .path
        .parent()
        .ok_or("EXR source has no directory")?
        .join(&selected.file);
    result.fingerprint = selected.fingerprint.clone();
    Ok(Some(result))
}
