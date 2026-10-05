//! Per-viewer presentation navigation; never edits the document or render time.
use dear_imgui_rs::{Key, MouseButton, Ui};

#[derive(Default)]
pub(crate) struct Navigation {
    width: Option<f32>,
    offset: [f32; 2],
    drag: Option<MouseButton>,
}
impl Navigation {
    fn size(&self, fitted: [f32; 2]) -> [f32; 2] {
        self.width
            .map(|width| [width, width * fitted[1] / fitted[0].max(0.001)])
            .unwrap_or(fitted)
    }
    pub fn fit(&mut self) {
        *self = Self::default();
    }
    pub fn rect(&self, origin: [f32; 2], area: [f32; 2], fitted: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        let size = self.size(fitted);
        (
            std::array::from_fn(|i| origin[i] + (area[i] - size[i]) * 0.5 + self.offset[i]),
            size,
        )
    }
    /// Physical raster resolution follows image scale, capped at source pixels.
    /// Bounds are rounded outward so the returned tile covers every visible pixel.
    pub fn demand(
        &self,
        source: [u32; 2],
        area_pixels: [f32; 2],
        dpi: [f32; 2],
        divisor: u32,
    ) -> Option<([u32; 2], Option<fold_platform::desktop::PreviewRegion>)> {
        let area = std::array::from_fn(|i| area_pixels[i] / dpi[i].max(0.001));
        let fit = (area[0] / source[0] as f32).min(area[1] / source[1] as f32);
        let fitted = source.map(|n| n as f32 * fit);
        let (origin, size) = self.rect([0.; 2], area, fitted);
        let scale = (size[0] * dpi[0] / source[0] as f32).min(size[1] * dpi[1] / source[1] as f32)
            / divisor.max(1) as f32;
        let dimensions = source.map(|n| (n as f32 * scale.min(1.)).floor().max(1.) as u32);
        let mut low = [0; 2];
        let mut high = [0; 2];
        for i in 0..2 {
            if origin[i] >= area[i] || origin[i] + size[i] <= 0. {
                return None;
            }
            low[i] = ((-origin[i] / size[i]).clamp(0., 1.) * dimensions[i] as f32).floor() as u32;
            high[i] = (((area[i] - origin[i]) / size[i]).clamp(0., 1.) * dimensions[i] as f32)
                .ceil() as u32;
            high[i] = high[i].min(dimensions[i]);
            if high[i] <= low[i] {
                return None;
            }
        }
        let region = fold_platform::desktop::PreviewRegion {
            x: low[0],
            y: low[1],
            width: high[0] - low[0],
            height: high[1] - low[1],
        };
        Some((
            dimensions,
            (region != fold_platform::desktop::PreviewRegion::full(dimensions)).then_some(region),
        ))
    }
    fn zoom(&mut self, mouse: [f32; 2], center: [f32; 2], size: [f32; 2], factor: f32) {
        let factor = factor.clamp(1. / 32., 32.);
        let width = (size[0] * factor).clamp(1., 1_000_000.);
        let factor = width / size[0].max(0.001);
        self.width = Some(width);
        self.offset = std::array::from_fn(|i| {
            let anchor = mouse[i] - center[i];
            anchor + (self.offset[i] - anchor) * factor
        });
    }
    /// Run after authoring overlays: their active widgets own left gestures.
    pub fn input(&mut self, ui: &Ui, origin: [f32; 2], area: [f32; 2], fitted: [f32; 2]) {
        if ui.is_window_focused_with_flags(dear_imgui_rs::FocusedFlags::ROOT_AND_CHILD_WINDOWS)
            && !ui.io().want_text_input()
            && !ui.is_any_item_active()
            && ui.is_key_pressed(Key::F)
        {
            self.fit();
            return;
        }
        let end = std::array::from_fn(|i| origin[i] + area[i]);
        let hovered = ui.is_window_hovered() && ui.is_mouse_hovering_rect(origin, end);
        if hovered && !ui.is_any_item_active() {
            for button in [MouseButton::Middle, MouseButton::Left] {
                if ui.is_mouse_clicked(button)
                    && (button == MouseButton::Middle || !ui.is_any_item_hovered())
                {
                    self.drag = Some(button);
                    self.width = Some(self.size(fitted)[0]);
                    break;
                }
            }
            if self.drag.is_none() && ui.io().mouse_wheel() != 0. {
                self.zoom(
                    ui.io().mouse_pos(),
                    std::array::from_fn(|i| origin[i] + area[i] * 0.5),
                    self.size(fitted),
                    1.2_f32.powf(ui.io().mouse_wheel()),
                );
            }
        }
        if let Some(button) = self.drag {
            if ui.is_mouse_down(button) && !ui.is_mouse_clicked(button) {
                let delta = ui.io().mouse_delta();
                for (offset, delta) in self.offset.iter_mut().zip(delta) {
                    *offset += delta;
                }
            } else if !ui.is_mouse_down(button) {
                self.drag = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demand_tracks_zoom_dpi_quality_and_offscreen_navigation() {
        use fold_platform::desktop::PreviewRegion as R;
        let mut view = Navigation::default();
        let source = [1920, 1080];
        let area = [960., 540.];
        assert_eq!(
            view.demand(source, area, [1.; 2], 1),
            Some(([960, 540], None))
        );
        view.zoom([480., 270.], [480., 270.], area, 2.);
        assert_eq!(
            view.demand(source, area, [1.; 2], 1),
            Some((
                source,
                Some(R {
                    x: 480,
                    y: 270,
                    width: 960,
                    height: 540
                })
            ))
        );
        assert_eq!(
            view.demand(source, area, [1.; 2], 2),
            Some((
                [960, 540],
                Some(R {
                    x: 240,
                    y: 135,
                    width: 480,
                    height: 270
                })
            ))
        );
        view.offset = [4000., 0.];
        assert!(view.demand(source, area, [1.; 2], 1).is_none());
        view.fit();
        assert_eq!(
            view.demand(source, [1920., 1080.], [2.; 2], 1),
            Some((source, None))
        );
        view.zoom([240., 135.], [480., 270.], area, 4.);
        let (dimensions, region) = view.demand(source, area, [1.; 2], 1).unwrap();
        assert_eq!(
            dimensions, source,
            "do not supersample past source resolution"
        );
        let region = region.unwrap();
        assert!(
            region.width >= 480
                && region.width <= 481
                && region.height >= 270
                && region.height <= 271,
            "outward bounds cover fractional source pixels"
        );
        assert!(
            region.x < 720 && region.y < 405,
            "cursor anchoring changes the crop"
        );
    }
    #[test]
    fn zoom_preserves_cursor_anchor_and_fit_resets_pan() {
        let mut view = Navigation::default();
        let (origin, size) = view.rect([10., 20.], [800., 600.], [800., 400.]);
        assert_eq!(origin, [10., 120.]);
        let mouse = [210., 220.];
        view.zoom(mouse, [410., 320.], size, 2.);
        let (zoomed, size) = view.rect([10., 20.], [800., 600.], [800., 400.]);
        assert_eq!(size, [1600., 800.]);
        for i in 0..2 {
            assert_eq!(mouse[i] - zoomed[i], 2. * (mouse[i] - origin[i]));
        }
        assert_eq!(view.rect([0.; 2], [400., 300.], [400., 200.]).1, size);
        view.fit();
        assert_eq!(
            view.rect([0.; 2], [400., 300.], [400., 200.]),
            ([0., 50.], [400., 200.])
        );
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    #[test]
    fn both_buttons_pan_but_left_handles_keep_priority_and_f_fits() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([800., 600.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut view = Navigation::default();
        let frame = |context: &mut dear_imgui_rs::Context, view: &mut Navigation, handle: bool| {
            let ui = context.frame();
            ui.window("Navigation")
                .position([0.; 2], dear_imgui_rs::Condition::Always)
                .size([600., 500.], dear_imgui_rs::Condition::Always)
                .build(|| {
                    if handle {
                        ui.set_cursor_screen_pos([100., 100.]);
                        ui.invisible_button("handle", [40., 40.]);
                    }
                    view.input(ui, [10., 30.], [580., 450.], [580., 290.]);
                });
            context.end_frame();
        };
        for button in [MouseButton::Left, MouseButton::Middle] {
            view.fit();
            context.io_mut().add_mouse_pos_event([200., 200.]);
            frame(&mut context, &mut view, false);
            frame(&mut context, &mut view, false);
            context.io_mut().add_mouse_button_event(button, true);
            frame(&mut context, &mut view, false);
            assert_eq!(view.offset, [0.; 2], "press must not jump");
            context.io_mut().add_mouse_pos_event([240., 220.]);
            frame(&mut context, &mut view, false);
            assert_eq!(view.offset, [40., 20.]);
            context.io_mut().add_mouse_button_event(button, false);
            frame(&mut context, &mut view, false);
            assert!(view.drag.is_none());
        }
        context.io_mut().add_key_event(Key::F, true);
        frame(&mut context, &mut view, false);
        assert!(view.width.is_none());
        assert_eq!(view.offset, [0.; 2]);
        context.io_mut().add_key_event(Key::F, false);
        context.io_mut().add_mouse_pos_event([120., 120.]);
        frame(&mut context, &mut view, true);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        frame(&mut context, &mut view, true);
        context.io_mut().add_mouse_pos_event([140., 130.]);
        frame(&mut context, &mut view, true);
        assert!(view.drag.is_none());
        assert_eq!(view.offset, [0.; 2]);
    }
}
