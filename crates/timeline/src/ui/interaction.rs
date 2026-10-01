//! UI-independent canvas geometry and edit gestures. Tests exercise the same
//! quantization, snapping, and proposals used by the actual ImGui canvas.
use crate::{ClipEdit, Sequence, SequenceEdit};
use fold_foundation::{ObjectId, Time};
use fold_project::Revision;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}
impl Rect {
    pub fn contains(self, p: [f32; 2]) -> bool {
        p[0] >= self.min[0] && p[0] < self.max[0] && p[1] >= self.min[1] && p[1] < self.max[1]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Move,
    Left,
    Right,
}
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
pub fn handle(rect: Rect, point: [f32; 2], scale: f32) -> Handle {
    let edge = (6.0 * scale).min((rect.max[0] - rect.min[0]) / 3.0);
    if point[0] - rect.min[0] < edge {
        Handle::Left
    } else if rect.max[0] - point[0] < edge {
        Handle::Right
    } else {
        Handle::Move
    }
}
pub struct Drag {
    pub base: Revision,
    pub original: Sequence,
    pub clip: ObjectId,
    pub linked: bool,
    pub handle: Handle,
    pub grab_frame: f64,
}
impl Drag {
    pub fn proposal(
        &self,
        pointer_frame: f64,
        track: ObjectId,
        snap: Option<(Time, f64)>,
    ) -> Result<SequenceEdit, String> {
        let clip = self
            .original
            .clips
            .iter()
            .find(|c| c.id == self.clip)
            .ok_or("missing dragged clip")?;
        let delta = time(
            (pointer_frame - self.grab_frame).round() as i64,
            self.original.rate,
        )?;
        let mut start = clip.start;
        let mut end = clip.end()?;
        if self.handle != Handle::Right {
            start = start.checked_add(delta).map_err(|e| e.to_string())?;
        }
        if self.handle != Handle::Left {
            end = end.checked_add(delta).map_err(|e| e.to_string())?;
        }
        if let Some((playhead, tolerance)) = snap {
            let excluded = self.original.linked_ids(self.clip, self.linked)?;
            let mut anchors = vec![Time::ZERO, playhead];
            for other in self
                .original
                .clips
                .iter()
                .filter(|c| !excluded.contains(&c.id))
            {
                anchors.push(other.start);
                anchors.push(other.end()?);
            }
            let edges = match self.handle {
                Handle::Left => vec![start],
                Handle::Right => vec![end],
                Handle::Move => vec![start, end],
            };
            let mut best: Option<(f64, Time)> = None;
            for edge in edges {
                for anchor in &anchors {
                    let offset = anchor.checked_sub(edge).map_err(|e| e.to_string())?;
                    let distance = frame(offset, self.original.rate).abs();
                    if distance <= tolerance && best.as_ref().is_none_or(|(d, _)| distance < *d) {
                        best = Some((distance, offset));
                    }
                }
            }
            if let Some((_, offset)) = best {
                if self.handle != Handle::Right {
                    start = start.checked_add(offset).map_err(|e| e.to_string())?;
                }
                if self.handle != Handle::Left {
                    end = end.checked_add(offset).map_err(|e| e.to_string())?;
                }
            }
        }
        let edit = if self.handle == Handle::Move {
            ClipEdit::MoveTo { start, track }
        } else {
            ClipEdit::Trim { start, end }
        };
        Ok(SequenceEdit::Clip {
            id: self.clip,
            linked: self.linked,
            edit,
        })
    }
}
