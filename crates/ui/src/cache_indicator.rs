//! Presentation-cache coverage: source identity must match before showing color.
use crate::sdk::imgui::Ui;
use fold_platform::desktop::PreviewKey;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Coverage {
    Partial,
    Ready,
}
pub fn frames(
    keys: &[PreviewKey],
    requested: &PreviewKey,
) -> std::collections::BTreeMap<u32, Coverage> {
    let mut result = std::collections::BTreeMap::new();
    for key in keys {
        if key.content != requested.content
            || key.output != requested.output
            || key.target.map(|t| t.0) != requested.target.map(|t| t.0)
            || key.view != requested.view
            || key.channels != requested.channels
        {
            continue;
        }
        if key.frame == requested.frame && key.target != requested.target {
            continue;
        }
        let coverage = if key.dimensions == requested.dimensions && key.region == requested.region {
            Coverage::Ready
        } else {
            Coverage::Partial
        };
        result
            .entry(key.frame)
            .and_modify(|old| *old = std::cmp::max(*old, coverage))
            .or_insert(coverage);
    }
    result
}
pub fn color(coverage: Coverage) -> [f32; 4] {
    match coverage {
        Coverage::Ready => [0.30, 0.70, 0.52, 1.],
        Coverage::Partial => [0.75, 0.59, 0.30, 1.],
    }
}
pub fn strip(
    ui: &Ui,
    origin: [f32; 2],
    width: f32,
    first: f64,
    pixels_per_frame: f64,
    frames: &std::collections::BTreeMap<u32, Coverage>,
) {
    let draw = ui.get_window_draw_list();
    strip_on(&draw, origin, width, first, pixels_per_frame, frames);
}
pub fn strip_on(
    draw: &crate::sdk::imgui::DrawListMut<'_>,
    origin: [f32; 2],
    width: f32,
    first: f64,
    pixels_per_frame: f64,
    frames: &std::collections::BTreeMap<u32, Coverage>,
) {
    for (&frame, &coverage) in frames {
        let x = origin[0] + ((f64::from(frame) - first) * pixels_per_frame) as f32;
        let right = (x + pixels_per_frame as f32).min(origin[0] + width);
        if right > origin[0] && x < origin[0] + width {
            draw.add_rect(
                [x.max(origin[0]), origin[1]],
                [right, origin[1] + 2.],
                color(coverage),
            )
            .filled(true)
            .build();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_settings_and_eviction_never_claim_ready() {
        let key = PreviewKey {
            target: None,
            region: None,
            output: "video".into(),
            content: "a".into(),
            frame: 12,
            dimensions: [1920, 1080],
            view: 1,
            channels: Default::default(),
        };
        assert_eq!(frames(&[key.clone()], &key)[&12], Coverage::Ready);
        let mut changed = key.clone();
        changed.content = "b".into();
        assert!(frames(&[key.clone()], &changed).is_empty());
        changed = key.clone();
        changed.dimensions = [960, 540];
        assert_eq!(frames(&[changed], &key)[&12], Coverage::Partial);
        assert!(frames(&[], &key).is_empty());
        let mut port = key.clone();
        port.output = "depth".into();
        let mut transform = key.clone();
        transform.view = 2;
        let mut source = key.clone();
        source.target = Some((
            fold_foundation::DocumentId::new(),
            fold_foundation::Time::ZERO,
        ));
        for changed in [port, transform, source] {
            assert!(frames(&[changed], &key).is_empty());
        }
    }
}
