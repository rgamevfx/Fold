//! Time navigation shared by clip and animation editors.
use fold_foundation::Time;
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub first: f64,
    pub pixels_per_frame: f64,
    pub scroll_y: f32,
}
impl Default for View {
    fn default() -> Self {
        Self {
            first: 0.0,
            pixels_per_frame: 5.0,
            scroll_y: 0.0,
        }
    }
}
impl View {
    pub fn frame_at(&self, x: f32, origin: f32) -> f64 {
        self.first + f64::from(x - origin) / self.pixels_per_frame
    }
    pub fn x(&self, frame: f64, origin: f32) -> f32 {
        origin + ((frame - self.first) * self.pixels_per_frame) as f32
    }
    pub fn zoom(&mut self, factor: f64, x: f32, origin: f32) {
        let anchor = self.frame_at(x, origin);
        self.pixels_per_frame = (self.pixels_per_frame * factor).clamp(0.05, 80.0);
        self.first = (anchor - f64::from(x - origin) / self.pixels_per_frame).max(0.0);
    }
}
pub fn frame(time: Time, rate: [u32; 2]) -> f64 {
    time.numerator() as f64 / f64::from(time.denominator()) * f64::from(rate[0])
        / f64::from(rate[1])
}
pub fn time(frame: i64, rate: [u32; 2]) -> Result<Time, String> {
    Time::new(frame, rate[0])
        .and_then(|t| t.checked_scale(i64::from(rate[1]), 1))
        .map_err(|e| e.to_string())
}

/// Non-drop frame timecode. Storage remains exact at fractional rates.
pub fn timecode(frame: i64, rate: [u32; 2]) -> String {
    let fps = (f64::from(rate[0]) / f64::from(rate[1])).round().max(1.) as i64;
    let sign = if frame < 0 { "−" } else { "" };
    let f = frame.abs();
    format!(
        "{sign}{:02}:{:02}:{:02}:{:02}",
        f / fps / 3600,
        (f / fps / 60) % 60,
        (f / fps) % 60,
        f % fps
    )
}
pub fn ruler_step(pixels_per_frame: f64, rate: [u32; 2], spacing: f64) -> f64 {
    let fps = (f64::from(rate[0]) / f64::from(rate[1])).round().max(1.);
    [
        1.,
        2.,
        5.,
        10.,
        fps,
        2. * fps,
        5. * fps,
        10. * fps,
        30. * fps,
        60. * fps,
        120. * fps,
        300. * fps,
        600. * fps,
    ]
    .into_iter()
    .filter(|s| *s * pixels_per_frame >= spacing)
    .min_by(f64::total_cmp)
    .unwrap_or(600. * fps)
}
