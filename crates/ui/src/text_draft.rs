//! Local text edits retain artist input when committed text changes externally.
pub struct TextDraft {
    pub text: String,
    original: String,
}
impl TextDraft {
    pub fn new(current: &str) -> Self {
        Self {
            text: current.into(),
            original: current.into(),
        }
    }
    pub fn sync(&mut self, current: &str) {
        if self.text == self.original || self.text == current {
            self.reset(current);
        }
    }
    pub fn conflicts(&self, current: &str) -> bool {
        self.original != current
    }
    pub fn reset(&mut self, current: &str) {
        self.text = current.into();
        self.original = current.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_edits_preserve_dirty_text_until_reload_or_publication() {
        let mut draft = TextDraft::new("Box");
        draft.sync("Sphere");
        assert_eq!(draft.text, "Sphere");
        draft.text = "Hero".into();
        draft.sync("Agent name");
        assert_eq!(draft.text, "Hero");
        assert!(draft.conflicts("Agent name"));
        draft.reset("Agent name");
        assert_eq!(draft.text, "Agent name");
        assert!(!draft.conflicts("Agent name"));
        draft.text = "Published".into();
        draft.sync("Published");
        assert!(!draft.conflicts("Published"));
        draft.sync("Undo");
        assert_eq!(draft.text, "Undo");
    }
}
