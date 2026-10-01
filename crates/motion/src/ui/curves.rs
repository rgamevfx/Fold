//! Selected-track curve/dope view. Dragging a key snaps to an exact frame;
//! the inspector's rational fields retain subframe editing.
use crate::{animation::Track, fields::Datum};
use fold_foundation::ObjectId;
use fold_ui::sdk::{
    GRAPH_COLORS,
    imgui::{MouseButton, Ui},
};
#[derive(Default)]
pub struct Curves {
    node: Option<ObjectId>,
    drag: Option<usize>,
}
impl Curves {
    pub fn draw(
        &mut self,
        ui: &Ui,
        node: ObjectId,
        track: &mut Track,
        info: &fold_media::VideoInfo,
    ) -> (bool, bool) {
        if self.node != Some(node) {
            self.node = Some(node);
            self.drag = None;
        }
        let origin = ui.cursor_screen_pos();
        let size = [ui.content_region_avail()[0].max(60.), 120.];
        ui.invisible_button("motion-keyframe-curve", size);
        let draw = ui.get_window_draw_list();
        draw.add_rect(
            origin,
            [origin[0] + size[0], origin[1] + size[1]],
            GRAPH_COLORS.grid,
        )
        .filled(true)
        .build();
        let scalar = |v: &Datum| match v {
            Datum::Scalar(v) => *v,
            Datum::Vector(v) => v[0],
            Datum::Color(v) => v[0],
            _ => 0.,
        };
        let min = track
            .keys
            .iter()
            .map(|k| scalar(&k.value))
            .fold(0., f64::min);
        let max = track
            .keys
            .iter()
            .map(|k| scalar(&k.value))
            .fold(1., f64::max);
        let range = (max - min).max(1.);
        let duration = info.time(info.frames).unwrap();
        let seconds = duration.numerator() as f64 / duration.denominator() as f64;
        let point = |time: fold_foundation::Time, value: &Datum| {
            [
                origin[0]
                    + ((time.numerator() as f64 / time.denominator() as f64) / seconds) as f32
                        * size[0],
                origin[1] + size[1] - 8. - ((scalar(value) - min) / range) as f32 * (size[1] - 16.),
            ]
        };
        let mut previous = None;
        if track.validate().is_ok() {
            for i in 0..=64 {
                let time = duration.checked_scale(i, 64).unwrap();
                if let Ok(value) = track.sample(time) {
                    let p = point(time, &value);
                    if let Some(previous) = previous {
                        draw.add_line(previous, p, GRAPH_COLORS.image)
                            .thickness(1.5)
                            .build();
                    }
                    previous = Some(p);
                }
            }
        }
        for key in &track.keys {
            let p = point(key.time, &key.value);
            draw.add_circle(p, 5., GRAPH_COLORS.selected)
                .filled(true)
                .build();
        }
        if ui.is_item_activated() {
            let mouse = ui.io().mouse_pos();
            self.drag = track
                .keys
                .iter()
                .enumerate()
                .filter_map(|(i, k)| {
                    let p = point(k.time, &k.value);
                    let distance = (mouse[0] - p[0]).hypot(mouse[1] - p[1]);
                    (distance < 12.).then_some((i, distance))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
        }
        let mut changed = false;
        if ui.is_item_active()
            && let Some(i) = self.drag
        {
            let frame = (((ui.io().mouse_pos()[0] - origin[0]) / size[0]).clamp(0., 1.)
                * info.frames.saturating_sub(1) as f32)
                .round() as u32;
            let time = info.time(frame).unwrap();
            if i < track.keys.len()
                && track.keys[i].time != time
                && !track
                    .keys
                    .iter()
                    .enumerate()
                    .any(|(j, k)| i != j && k.time == time)
            {
                track.keys[i].time = time;
                track.keys.sort_by_key(|k| k.time);
                self.drag = track.keys.iter().position(|k| k.time == time);
                changed = true;
            }
        }
        let finished = self.drag.is_some() && ui.is_mouse_released(MouseButton::Left);
        if finished {
            self.drag = None;
            track.keys.sort_by_key(|k| k.time);
        }
        ui.text_disabled("Drag dots to retime • exact/subframe times below • vector/color curve shows first component");
        (changed, finished)
    }
}
