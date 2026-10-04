//! Contextual key and tangent tools; no permanent toolbar expansion.
use super::{Channel, Editor, Response};
use crate::sdk::imgui::Ui;
use fold_animation::{Interpolation, Tangents};
use fold_foundation::Time;

impl Editor {
    pub(super) fn key_menu(&mut self, ui: &Ui, channels: &mut [Channel], response: &mut Response) {
        let _disabled = self.selected.is_empty().then(|| ui.begin_disabled());
        for (label, interpolation) in [
            ("Hold", Interpolation::Hold),
            ("Linear", Interpolation::Linear),
            ("Bézier", Interpolation::Bezier),
        ] {
            if ui.menu_item(label) {
                for c in channels.iter_mut() {
                    for k in &mut c.curve.keys {
                        if self.selected.contains(&k.id) {
                            k.interpolation = interpolation;
                        }
                    }
                }
                response.edit.changed = true;
                response.edit.finished = true;
            }
        }
        for (label, tangents) in [
            ("Automatic tangents", Tangents::Auto),
            ("Linked tangents", Tangents::Linked),
            ("Broken tangents", Tangents::Broken),
        ] {
            if ui.menu_item(label) {
                for c in channels.iter_mut() {
                    for i in 0..c.curve.keys.len() {
                        if self.selected.contains(&c.curve.keys[i].id) {
                            let incoming = c.curve.handle(i, true);
                            let outgoing = c.curve.handle(i, false);
                            c.curve.keys[i].incoming = incoming;
                            c.curve.keys[i].outgoing = outgoing;
                            c.curve.keys[i].tangents = tangents;
                            if tangents == Tangents::Linked {
                                c.curve.set_handle(i, false, outgoing);
                            }
                        }
                    }
                }
                response.edit.changed = true;
                response.edit.finished = true;
            }
        }
        if self.selected.len() == 1 {
            for c in channels.iter_mut() {
                if let Some(i) = c
                    .curve
                    .keys
                    .iter()
                    .position(|k| self.selected.contains(&k.id))
                {
                    let mut value = c.curve.keys[i].value;
                    let changed = ui.input_double("Value", &mut value) && value.is_finite();
                    response.edit.item(ui, changed);
                    if changed {
                        c.curve.keys[i].value = value;
                        self.numeric_edit = true;
                    }
                    if let Some(_menu) = ui.begin_menu("Tangent handles") {
                        for incoming in [true, false] {
                            let _id = ui.push_id(if incoming { "incoming" } else { "outgoing" });
                            ui.text(if incoming { "Incoming" } else { "Outgoing" });
                            let mut handle = c.curve.handle(i, incoming);
                            let fraction = crate::sdk::imgui::Drag::new("Time fraction")
                                .speed(0.01)
                                .range(0., 0.5)
                                .build(ui, &mut handle.fraction);
                            response.edit.item(ui, false);
                            let value = ui.input_double("Value offset", &mut handle.value);
                            response.edit.item(ui, false);
                            if (fraction || value)
                                && handle.value.is_finite()
                                && (0.0..=0.5).contains(&handle.fraction)
                            {
                                c.curve.set_handle(i, incoming, handle);
                                response.edit.changed = true;
                                self.numeric_edit = true;
                            }
                        }
                    }
                    let mut n = c.curve.keys[i].time.numerator();
                    let mut d = c.curve.keys[i].time.denominator();
                    let mut changed =
                        crate::sdk::imgui::Drag::new("Time numerator").build(ui, &mut n);
                    response.edit.item(ui, false);
                    changed |= crate::sdk::imgui::Drag::new("Time denominator")
                        .range(1, u32::MAX)
                        .build(ui, &mut d);
                    response.edit.item(ui, false);
                    if changed && let Ok(time) = Time::new(n, d) {
                        if !c
                            .curve
                            .keys
                            .iter()
                            .enumerate()
                            .any(|(j, k)| j != i && k.time == time)
                        {
                            c.curve.keys[i].time = time;
                            c.curve.keys.sort_by_key(|k| k.time);
                            response.edit.changed = true;
                            self.numeric_edit = true;
                        } else {
                            self.error = "Keyframe times must be unique.".into();
                        }
                    }
                    break;
                }
            }
        }
    }
}
