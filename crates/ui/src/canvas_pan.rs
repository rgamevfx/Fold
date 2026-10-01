//! Translate empty-canvas left gestures to native middle-drag before NewFrame.
//! Shift retains ordinary left input for box selection. Latch through release.
use dear_imgui_rs::MouseButton;

#[derive(Default)]
pub struct CanvasPan {
    shift: bool,
    middle: bool,
    active: bool,
    selecting: bool,
}
impl CanvasPan {
    pub fn shift(&mut self, pressed: bool) {
        self.shift = pressed;
    }
    pub fn middle(&mut self, pressed: bool) {
        self.middle = pressed;
    }
    /// The native editor binds Shift-box to groups only. Our Shift gesture
    /// means ordinary box selection, so consume that modifier for the gesture.
    pub fn effective_shift(&self) -> bool {
        self.shift && !self.selecting
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn left_button(&mut self, pressed: bool, over_background: bool) -> MouseButton {
        if pressed {
            self.active = !self.shift && !self.middle && over_background;
            self.selecting = self.shift && !self.middle && over_background;
        }
        let button = if self.active {
            MouseButton::Middle
        } else {
            MouseButton::Left
        };
        if !pressed {
            self.active = false;
            self.selecting = false;
        }
        button
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_pan_and_shift_selection_latch_until_release() {
        let mut pan = CanvasPan::default();
        assert_eq!(pan.left_button(true, false), MouseButton::Left);
        assert_eq!(pan.left_button(false, true), MouseButton::Left);
        assert_eq!(pan.left_button(true, true), MouseButton::Middle);
        pan.shift(true);
        assert_eq!(pan.left_button(false, false), MouseButton::Middle);
        assert_eq!(pan.left_button(true, true), MouseButton::Left);
        assert!(
            !pan.effective_shift(),
            "native group-only modifier is consumed"
        );
        assert_eq!(pan.left_button(false, true), MouseButton::Left);
        assert!(
            pan.effective_shift(),
            "physical modifier is restored on release"
        );
        assert_eq!(pan.left_button(true, true), MouseButton::Left);
        pan.shift(false);
        assert_eq!(pan.left_button(false, true), MouseButton::Left);
        pan.middle(true);
        assert_eq!(pan.left_button(true, true), MouseButton::Left);
        pan.reset();
        assert_eq!(pan.left_button(false, true), MouseButton::Left);
    }
}
