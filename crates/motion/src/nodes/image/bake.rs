//! Image Bake keeps the upstream image connection and cache state independent.
use super::*;
use fold_foundation::{DocumentId, ObjectId};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "resolution")]
    pub resolution: u32,
    #[serde(default)]
    pub start: u32,
    /// Zero means the document's end, exclusive.
    #[serde(default)]
    pub end: u32,
    #[serde(default)]
    pub format: fold_render::baked::Format,
    #[serde(default)]
    pub clamp_sdr: bool,
    #[serde(default)]
    pub bake: Option<fold_platform::image_bake::Reference>,
}
fn resolution() -> u32 {
    1024
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            resolution: resolution(),
            start: 0,
            end: 0,
            format: Default::default(),
            clamp_sdr: false,
            bake: None,
        }
    }
}
impl Settings {
    pub fn options(&self) -> String {
        format!(
            "v1:{}:{}:{}:{:?}:{}",
            self.resolution, self.start, self.end, self.format, self.clamp_sdr
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(16..=4096).contains(&self.resolution)
            || self.resolution % 4 != 0
            || self.end != 0 && self.end <= self.start
        {
            return Err(
                "Bake resolution must be a multiple of 4 in 16..4096; end must follow start".into(),
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Args {
    pub document: DocumentId,
    pub node: ObjectId,
    #[serde(default)]
    pub group: Option<ObjectId>,
}
impl Args {
    pub fn progress_key(&self) -> String {
        format!("{:?}/{:?}/{:?}", self.document, self.group, self.node)
    }
}
pub(super) fn definition() -> Definition {
    let mut def = super::definition(
        "fold.motion.image_bake",
        "Image Bake",
        "Image",
        vec![Socket::required("image", Kind::Image)],
        vec![("image", Kind::Image)],
        |e, n| {
            let settings = n.settings::<Settings>()?;
            settings.validate()?;
            let mut images = images(e, n)?.as_ref().clone();
            if let Some(bake) = &settings.bake {
                if bake.options != settings.options() {
                    return Err("Texture bake settings changed; rebuild or clear it".into());
                }
                for request in &mut images {
                    request.bake = Some(bake.clone());
                }
            }
            Ok(output("image", Value::Image(Arc::new(images))))
        },
    );
    def.defaults = || serde_json::to_value(Settings::default()).unwrap();
    def.description = "Prepare mipmapped, GPU-native textures while preserving the input. Bake/Rebuild runs in the background; Clear returns to the original. Auto preserves HDR as Float16; BC7 is a lossy SDR cache. Resolution is the longest edge, end is exclusive (0 means document end). Missing/stale/out-of-range bakes are explicit errors. Public worker command: fold.app.image-bake {document,node,group?}.";
    def.validate = |n| n.settings::<Settings>()?.validate();
    def
}
pub fn requests(
    motion: &crate::Motion,
    node: ObjectId,
    time: fold_foundation::Time,
    cancel: &fold_media::Cancel,
) -> Result<Vec<Request>, String> {
    let n = motion
        .graph
        .nodes
        .iter()
        .find(|n| n.id == node && n.kind == "fold.motion.image_bake")
        .ok_or("Select an Image Bake node")?;
    let mut evaluator = Evaluator::new(motion, time)?;
    evaluator.budget = crate::fields::Budget::cancellable(cancel);
    Ok(images(&mut evaluator, n)?.as_ref().clone())
}
