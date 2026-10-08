//! Portable display pixels for playback retention. Never a delivery source.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    Rgba8,
    Bc7,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Description {
    pub dimensions: [u32; 2],
    pub format: Format,
}
impl Description {
    pub fn layout(&self) -> Result<([u32; 2], u32, u32), String> {
        let [w, h] = self.dimensions;
        if w == 0 || h == 0 || w > 32768 || h > 32768 {
            return Err("Invalid presentation cache dimensions".into());
        }
        Ok(match self.format {
            Format::Rgba8 => ([w, h], w * 4, h),
            Format::Bc7 => (
                [w.div_ceil(4) * 4, h.div_ceil(4) * 4],
                w.div_ceil(4) * 16,
                h.div_ceil(4),
            ),
        })
    }
    pub fn bytes(&self) -> Result<usize, String> {
        let (_, row, rows) = self.layout()?;
        Ok(row as usize * rows as usize)
    }
}
pub struct Pixels {
    pub description: Description,
    pub data: Vec<u8>,
}
impl Pixels {
    pub fn validate(&self) -> Result<(), String> {
        if self.description.bytes()? != self.data.len() {
            return Err("Invalid presentation cache payload length".into());
        }
        Ok(())
    }
}
