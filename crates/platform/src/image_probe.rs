//! Bounded, cancellable scene sampling for artist tools. Pixels are immutable;
//! display RGB and straight scene-linear sampling are explicitly separate.
use fold_foundation::Time;
use fold_project::DocumentRef;
pub struct Request {
    pub source: DocumentRef,
    pub time: Time,
    pub dimensions: [u32; 2],
    pub data_display: bool,
}
pub struct Image {
    pub display: fold_render::DisplayFrame,
    pub samples: Vec<[f32; 4]>,
    pub storage: std::sync::Arc<fold_media::budget::Lease>,
}
pub type Receiver = std::sync::mpsc::Receiver<Result<Image, String>>;
impl Image {
    /// Alpha-weighted 3×3 sample: transparent padding cannot bias the selected color.
    pub fn sample(&self, uv: [f32; 2]) -> Result<[f64; 3], String> {
        let [w, h] = self.display.dimensions();
        let x = (uv[0] * w as f32).floor() as i32;
        let y = (uv[1] * h as f32).floor() as i32;
        let mut rgb = [0.; 3];
        let mut alpha = 0.;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let px = x + dx;
                let py = y + dy;
                if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                    continue;
                }
                let p = self.samples[(py as u32 * w + px as u32) as usize];
                if p[3] > 0.000001 {
                    for i in 0..3 {
                        rgb[i] += f64::from(p[i]);
                    }
                    alpha += f64::from(p[3]);
                }
            }
        }
        if alpha <= 0.000001 {
            return Err("Choose a visible, nontransparent pixel".into());
        }
        Ok(rgb.map(|v| v / alpha))
    }
}
