//! Decode layout facts, independent of authored dimensions and color assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoFormat {
    pub pixel_format: &'static str,
    pub depth: u32,
    pub subsampling: [u32; 2],
    pub alpha: bool,
    /// Planes are G/B/R full-range encoded samples rather than Y/U/V.
    pub rgb: bool,
    /// Chroma origin in half-luma-sample units.
    pub chroma_origin: [u32; 2],
    pub full_range: bool,
    pub nvdec_compatible: bool,
}
impl Default for VideoFormat {
    fn default() -> Self {
        Self {
            pixel_format: "yuv420p",
            depth: 8,
            subsampling: [2, 2],
            alpha: false,
            rgb: false,
            chroma_origin: [0, 1],
            full_range: false,
            nvdec_compatible: true,
        }
    }
}
impl VideoFormat {
    pub fn validate(self) -> Result<(), fold_foundation::Failure> {
        if ![8, 10, 12].contains(&self.depth)
            || ![[2, 2], [2, 1], [1, 1]].contains(&self.subsampling)
            || self.chroma_origin.iter().any(|v| *v > 1)
            || (self.rgb && (self.subsampling != [1, 1] || !self.full_range))
        {
            return Err("invalid native sample layout".into());
        }
        Ok(())
    }
    pub fn sample_bytes(self) -> u32 {
        if self.depth > 8 { 2 } else { 1 }
    }
    pub fn bytes(self, [w, h]: [u32; 2]) -> u64 {
        (u64::from(w) * u64::from(h) * if self.alpha { 2 } else { 1 }
            + 2 * u64::from(w.div_ceil(self.subsampling[0]))
                * u64::from(h.div_ceil(self.subsampling[1])))
            * u64::from(self.sample_bytes())
    }
    pub(crate) fn parse(s: &serde_json::Value) -> Result<Self, fold_foundation::Failure> {
        let codec = s["codec_name"].as_str().unwrap_or("");
        if !["h264", "hevc", "prores", "dnxhd"].contains(&codec) {
            return Err(format!("unsupported video codec: {codec}").into());
        }
        let pix = s["pix_fmt"].as_str().unwrap_or("");
        let rgb = pix.starts_with("gbrp");
        let (pixel_format, depth, subsampling, alpha) = match pix {
            "gbrp" => ("gbrp", 8, [1, 1], false),
            "gbrp10le" => ("gbrp10le", 10, [1, 1], false),
            "gbrp12le" => ("gbrp12le", 12, [1, 1], false),
            "yuv420p" => ("yuv420p", 8, [2, 2], false),
            "yuv420p10le" => ("yuv420p10le", 10, [2, 2], false),
            "yuv422p" => ("yuv422p", 8, [2, 1], false),
            "yuv422p10le" => ("yuv422p10le", 10, [2, 1], false),
            "yuv422p12le" => ("yuv422p12le", 12, [2, 1], false),
            "yuv444p" => ("yuv444p", 8, [1, 1], false),
            "yuv444p10le" => ("yuv444p10le", 10, [1, 1], false),
            "yuv444p12le" => ("yuv444p12le", 12, [1, 1], false),
            "yuva444p10le" => ("yuva444p10le", 10, [1, 1], true),
            "yuva444p12le" => ("yuva444p12le", 12, [1, 1], true),
            _ => return Err(format!("unsupported video pixel format: {pix}").into()),
        };
        for key in ["color_space", "color_transfer", "color_primaries"] {
            if s[key].as_str().is_some_and(|v| {
                !["unknown", "bt709"].contains(&v) && !(rgb && key == "color_space" && v == "gbr")
            }) {
                return Err(format!("unsupported SDR {key}: {}", s[key]).into());
            }
        }
        if s["sample_aspect_ratio"]
            .as_str()
            .is_some_and(|v| !["1:1", "N/A", "0:1"].contains(&v))
        {
            return Err("non-square video pixels are not supported".into());
        }
        if s["field_order"]
            .as_str()
            .is_some_and(|v| !["unknown", "progressive"].contains(&v))
        {
            return Err("interlaced video is not supported".into());
        }
        let full_range = match s["color_range"].as_str() {
            Some("pc") => true,
            None | Some("unknown") => rgb,
            Some("tv") => false,
            _ => return Err("unsupported video range".into()),
        };
        if rgb && !full_range {
            return Err("limited-range RGB video is not supported".into());
        }
        let chroma_origin = match s["chroma_location"].as_str() {
            None | Some("unspecified" | "left") => [0, if subsampling[1] == 2 { 1 } else { 0 }],
            Some("center") => [
                if subsampling[0] == 2 { 1 } else { 0 },
                if subsampling[1] == 2 { 1 } else { 0 },
            ],
            Some("topleft") => [0, 0],
            _ => return Err("unsupported chroma location".into()),
        };
        Ok(Self {
            pixel_format,
            depth,
            subsampling,
            alpha,
            rgb,
            chroma_origin,
            full_range,
            nvdec_compatible: (codec == "h264" && depth == 8 || codec == "hevc" && depth <= 10)
                && subsampling == [2, 2]
                && !alpha
                && !full_range
                && chroma_origin == [0, 1]
                && ["color_space", "color_transfer", "color_primaries"]
                    .iter()
                    .all(|k| s[*k] == "bt709")
                && s["color_range"] == "tv"
                && s["chroma_location"] == "left",
        })
    }
}
