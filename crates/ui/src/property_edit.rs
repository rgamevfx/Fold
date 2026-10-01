//! Shared lifecycle for property controls. Feature panels translate the response
//! into their own transactional commands; this module never owns project state.
use dear_imgui_rs::{Drag, DragFlags, Key, Ui};
use fold_foundation::{DocumentId, ObjectId};

/// Stable scope independent of visible labels and object addresses. Property
/// paths are pushed inside this scope; panel identifies the panel instance.
pub struct UiId<'a> {
    pub package: &'a str,
    pub panel: &'a str,
    pub document: DocumentId,
    pub object: ObjectId,
}
impl UiId<'_> {
    pub fn scope<'a>(&self, ui: &'a Ui) -> impl Drop + 'a + use<'a> {
        ui.push_id(&format!(
            "{}:{}:{:?}:{:?}",
            self.package, self.panel, self.document, self.object
        ))
    }
}

/// Minimal metadata for scalar properties actually used by the built-in
/// inspectors. It is not a public plugin schema or an animation-stack model.
pub struct NumericProperty<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub unit: &'a str,
    pub speed: f32,
    pub range: Option<[f64; 2]>,
    pub default: Option<f64>,
}
impl NumericProperty<'_> {
    pub fn draw(&self, ui: &Ui, value: &mut f64, response: &mut EditResponse) {
        let _id = ui.push_id(self.id);
        let mut drag = Drag::new(format!("{}###value", self.label)).speed(self.speed);
        if let Some([min, max]) = self.range {
            drag = drag.range(min, max).flags(DragFlags::ALWAYS_CLAMP);
        }
        let before = *value;
        let changed = drag.build(ui, value);
        if !value.is_finite() {
            *value = before;
        }
        response.item(ui, changed && *value != before);
        if !self.unit.is_empty() {
            ui.same_line();
            ui.text_disabled(self.unit);
        }
        if let Some(default) = self.default {
            ui.same_line();
            if ui.small_button("Reset###reset") && *value != default {
                *value = default;
                response.record(true, true, false, false);
            }
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct EditResponse {
    pub began: bool,
    pub changed: bool,
    /// Commit the gesture once, after applying this frame's changed value.
    pub finished: bool,
    /// Cancellation takes precedence over changed/finished.
    pub cancelled: bool,
}
impl EditResponse {
    /// Record the last drawn control. Discrete changes commit immediately;
    /// drags and text edits commit only when the edited control deactivates.
    pub fn item(&mut self, ui: &Ui, changed: bool) {
        self.record(
            changed,
            ui.is_item_activated(),
            ui.is_item_active(),
            ui.is_item_deactivated_after_edit(),
        );
    }

    /// Call within the owning inspector after drawing its controls. Only an
    /// existing edit is cancelled, so Escape cannot create a project operation.
    pub fn cancel_on_escape(&mut self, ui: &Ui, editing: bool) {
        self.cancelled = editing && ui.is_key_pressed(Key::Escape);
    }

    fn record(&mut self, changed: bool, activated: bool, active: bool, deactivated: bool) {
        self.began |= activated || (changed && !active);
        self.changed |= changed;
        self.finished |= deactivated || (changed && !active);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_identity_survives_label_changes_and_isolates_documents_and_objects() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = dear_imgui_rs::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([800., 200.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut identity = UiId {
            package: "test",
            panel: "inspector",
            document: DocumentId::new(),
            object: ObjectId::new(),
        };
        let ui = context.frame();
        ui.window("Properties").build(|| {
            let original = {
                let _scope = identity.scope(ui);
                let mut value = 3.;
                let mut response = EditResponse::default();
                NumericProperty {
                    id: "position.x",
                    label: "Position",
                    unit: "px",
                    speed: 1.,
                    range: None,
                    default: Some(0.),
                }
                .draw(ui, &mut value, &mut response);
                assert_eq!(value, 3.);
                assert_eq!(response, EditResponse::default());
                let _property = ui.push_id("position.x");
                assert_eq!(ui.get_id("Position###value"), ui.get_id("Renamed###value"));
                ui.get_id("Position###value")
            };
            let original_document = identity.document;
            identity.document = DocumentId::new();
            {
                let _scope = identity.scope(ui);
                let _property = ui.push_id("position.x");
                assert_ne!(original, ui.get_id("Position###value"));
            }
            identity.document = original_document;
            identity.object = ObjectId::new();
            let _scope = identity.scope(ui);
            let _property = ui.push_id("position.x");
            assert_ne!(original, ui.get_id("Position###value"));
        });
        drop(context.render_legacy());
    }

    #[test]
    fn drag_updates_preview_then_finishes_once_on_release() {
        let mut press = EditResponse::default();
        press.record(false, true, true, false);
        assert!(press.began && !press.changed && !press.finished);
        let mut drag = EditResponse::default();
        drag.record(true, false, true, false);
        assert!(drag.changed && !drag.finished);
        let mut release = EditResponse::default();
        release.record(false, false, false, true);
        assert!(!release.changed && release.finished);
        let mut idle = EditResponse::default();
        idle.record(false, false, false, false);
        assert_eq!(idle, EditResponse::default());
    }

    #[test]
    fn discrete_changes_commit_and_other_controls_do_not_erase_response() {
        let mut response = EditResponse::default();
        response.record(true, false, false, false);
        response.record(false, false, false, false);
        assert!(response.began && response.changed && response.finished);
    }
}
