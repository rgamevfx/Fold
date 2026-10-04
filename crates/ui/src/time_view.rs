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
