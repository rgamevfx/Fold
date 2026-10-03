//! Review preparation retains bounded intent, not textures or project content.
use fold_foundation::Time;
use fold_platform::desktop::{PlaybackRange, PreviewKey};

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Prepare,
    Cancel,
    PlayCached,
}

pub(crate) struct Plan {
    pub generation: u64,
    pub template: PreviewKey,
    pub rate: [u32; 2],
    pub range: PlaybackRange,
    pub start: u32,
    pub total: u32,
    pub count: u32,
    pub cursor: u32,
    pub cached_playing: bool,
    pub message: Option<String>,
}
impl Plan {
    pub fn new(
        generation: u64,
        template: PreviewKey,
        rate: [u32; 2],
        range: PlaybackRange,
        frames: u32,
        available: usize,
    ) -> Self {
        let (start, end) = range.bounds(frames);
        let bytes = template
            .dimensions
            .iter()
            .map(|n| u64::from(*n))
            .product::<u64>()
            * 4;
        let total = end - start + 1;
        // RGBA8 is the fallback: admission never assumes BC7 will pass quality.
        // Leave capacity for current-frame replacement and bound key bookkeeping.
        let count = total
            .min(512)
            .min((available as u64 / bytes.max(1)).saturating_sub(2) as u32);
        Self {
            generation,
            template,
            rate,
            range,
            start,
            total,
            count,
            cursor: 0,
            cached_playing: false,
            message: None,
        }
    }
    pub fn key(&self, offset: u32) -> PreviewKey {
        let frame = self.start + offset;
        PreviewKey {
            frame,
            target: self.template.target.map(|(id, _)| {
                (
                    id,
                    Time::new(i64::from(frame) * i64::from(self.rate[1]), self.rate[0]).unwrap(),
                )
            }),
            ..self.template.clone()
        }
    }
    pub fn compatible(&self, generation: u64, key: &PreviewKey, range: PlaybackRange) -> bool {
        self.generation == generation
            && self.template.content == key.content
            && self.template.dimensions == key.dimensions
            && self.template.view == key.view
            && self.template.output == key.output
            && self.template.target.map(|t| t.0) == key.target.map(|t| t.0)
            && self.range == range
    }
    pub fn resident(&self, contains: impl Fn(&PreviewKey) -> bool) -> u32 {
        (0..self.count)
            .filter(|&offset| contains(&self.key(offset)))
            .count() as u32
    }
    pub fn next(&mut self, contains: impl Fn(&PreviewKey) -> bool) -> Option<PreviewKey> {
        if self.cached_playing || self.message.is_some() {
            return None;
        }
        while self.cursor < self.count && contains(&self.key(self.cursor)) {
            self.cursor += 1;
        }
        (self.cursor < self.count).then(|| self.key(self.cursor))
    }
    pub fn status(&self, resident: u32) -> String {
        if let Some(message) = &self.message {
            return message.clone();
        }
        if self.cached_playing {
            return "Cached Real-time preview".into();
        }
        if self.count < self.total {
            return format!(
                "Review: {resident}/{} resident · range exceeds preparation limit",
                self.total
            );
        }
        if self.cursor < self.count {
            format!("Preparing review: {resident}/{} resident", self.total)
        } else if resident == self.total {
            format!("Review ready: {resident}/{} resident", self.total)
        } else {
            format!(
                "Review: {resident}/{} resident · frames evicted",
                self.total
            )
        }
    }
    pub fn can_replay(&self, resident: u32) -> bool {
        self.message.is_none() && self.count == self.total && resident == self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(bytes: usize) -> Plan {
        Plan::new(
            1,
            PreviewKey {
                target: Some((fold_foundation::DocumentId::new(), Time::ZERO)),
                output: "video".into(),
                content: "scene".into(),
                frame: 0,
                dimensions: [16, 16],
                view: 1,
            },
            [30_000, 1001],
            PlaybackRange {
                start: Some(7),
                end: Some(11),
            },
            100,
            bytes,
        )
    }
    #[test]
    fn exact_range_and_conservative_pressure_limit() {
        let mut bounded = plan(4 * 1024);
        assert_eq!(bounded.count, 2);
        assert_eq!(bounded.total, 5);
        let key = bounded.next(|_| false).unwrap();
        assert_eq!(key.frame, 7);
        assert_eq!(key.target.unwrap().1, Time::new(7 * 1001, 30_000).unwrap());
        assert!(!bounded.can_replay(2));
        assert!(bounded.status(2).contains("limit"));
        assert_eq!(plan(1).count, 0);
    }
    #[test]
    fn completed_preparation_does_not_refill_evicted_frames_forever() {
        let mut plan = plan(7 * 1024);
        assert!(plan.next(|_| true).is_none());
        assert!(plan.can_replay(5));
        assert!(!plan.can_replay(4));
        assert!(plan.next(|_| false).is_none());
        assert!(plan.status(4).contains("evicted"));
    }
    #[test]
    fn seek_edit_port_color_and_resolution_interrupt_preparation() {
        let plan = plan(7 * 1024);
        assert!(plan.compatible(1, &plan.key(1), plan.range));
        assert!(!plan.compatible(2, &plan.key(1), plan.range));
        for key in [
            PreviewKey {
                content: "edit".into(),
                ..plan.key(1)
            },
            PreviewKey {
                output: "other".into(),
                ..plan.key(1)
            },
            PreviewKey {
                view: 2,
                ..plan.key(1)
            },
            PreviewKey {
                dimensions: [8, 8],
                ..plan.key(1)
            },
        ] {
            assert!(!plan.compatible(1, &key, plan.range));
        }
    }
}
