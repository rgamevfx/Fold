//! Review preparation retains bounded intent, not textures or project content.
use fold_foundation::Time;
use fold_platform::desktop::{PlaybackRange, PreviewKey};

/// Shared hot memory is divided across reviews; disk reservations are divided
/// only among reviews using that store. Composition work cannot shrink timeline disk allowance.
pub(crate) fn allowance(
    gpu: usize,
    ram: u64,
    disk: u64,
    viewers: usize,
    disk_viewers: usize,
) -> (usize, bool) {
    let lower = (ram / viewers.max(1) as u64).max(disk / disk_viewers.max(1) as u64) as usize;
    ((gpu / viewers.max(1)).max(lower), lower > 0)
}

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
    retained: std::collections::BTreeSet<u32>,
}
impl Plan {
    pub fn new(
        generation: u64,
        template: PreviewKey,
        rate: [u32; 2],
        range: PlaybackRange,
        frames: u32,
        available: usize,
        compressed: bool,
    ) -> Self {
        let (start, end) = range.bounds(frames);
        let [width, height] = template.image_dimensions();
        let bytes = if compressed {
            // Include a conservative metadata allowance for persistent entries.
            u64::from(width.div_ceil(4)) * u64::from(height.div_ceil(4)) * 16 + 1024
        } else {
            u64::from(width) * u64::from(height) * 4
        };
        let total = end - start + 1;
        // Leave space for frame replacement and bound metadata independently.
        let count = total
            .min(fold_platform::playback_cache::ENTRY_LIMIT as u32)
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
            retained: Default::default(),
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
            && self.template.region == key.region
            && self.template.view == key.view
            && self.template.channels == key.channels
            && self.template.output == key.output
            && self.template.target.map(|t| t.0) == key.target.map(|t| t.0)
            && self.range == range
    }
    pub fn contains(&self, key: &PreviewKey) -> bool {
        key.frame >= self.start
            && key.frame - self.start < self.count
            && *key == self.key(key.frame - self.start)
    }
    pub fn limit(&mut self, count: u32) {
        self.count = self.count.min(count);
        self.retained
            .retain(|frame| *frame - self.start < self.count);
    }
    pub fn update_retained(&mut self, key: &PreviewKey, present: bool) {
        if self.contains(key) {
            if present {
                self.retained.insert(key.frame);
            } else {
                self.retained.remove(&key.frame);
            }
        }
    }
    pub fn resident<'a>(&self, gpu_keys: impl Iterator<Item = &'a PreviewKey>) -> u32 {
        self.retained.len() as u32
            + gpu_keys
                .filter(|key| !self.retained.contains(&key.frame) && self.contains(key))
                .count() as u32
    }
    pub fn preparing_key(&self, key: &PreviewKey) -> bool {
        self.message.is_none()
            && !self.cached_playing
            && self.cursor < self.count
            && key.frame >= self.start
            && key.frame - self.start < self.count
            && *key == self.key(key.frame - self.start)
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
        self.message.is_none()
            && self.cursor == self.count
            && self.count == self.total
            && resident == self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(bytes: usize) -> Plan {
        Plan::new(
            1,
            PreviewKey {
                region: None,
                target: Some((fold_foundation::DocumentId::new(), Time::ZERO)),
                output: "video".into(),
                content: "scene".into(),
                frame: 0,
                dimensions: [16, 16],
                view: 1,
                channels: Default::default(),
            },
            [30_000, 1001],
            PlaybackRange {
                start: Some(7),
                end: Some(11),
            },
            100,
            bytes,
            false,
        )
    }
    #[test]
    fn independent_disk_store_keeps_its_allowance_when_another_review_opens() {
        let mib = 1024 * 1024;
        assert_eq!(
            allowance(256 * mib, 1024 * mib as u64, 20 * 1024 * mib as u64, 1, 1).0,
            20 * 1024 * mib
        );
        assert_eq!(
            allowance(256 * mib, 1024 * mib as u64, 20 * 1024 * mib as u64, 2, 1).0,
            20 * 1024 * mib
        );
        assert_eq!(
            allowance(256 * mib, 1024 * mib as u64, 20 * 1024 * mib as u64, 2, 2).0,
            10 * 1024 * mib
        );
        assert_eq!(
            allowance(256 * mib, 1024 * mib as u64, 0, 2, 1).0,
            512 * mib
        );
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
        assert!(
            !plan.can_replay(5),
            "GPU-only coverage cannot finish lower-tier preparation early"
        );
        assert!(plan.next(|_| true).is_none());
        assert!(plan.can_replay(5));
        assert!(!plan.can_replay(4));
        assert!(plan.next(|_| false).is_none());
        assert!(plan.status(4).contains("evicted"));
    }
    #[test]
    fn old_resident_frames_are_preserved_until_range_preparation_finishes() {
        let mut plan = plan(7 * 1024);
        let early = plan.key(0);
        plan.cursor = 4;
        assert!(plan.preparing_key(&early));
        let mut unrelated = early.clone();
        unrelated.view += 1;
        assert!(!plan.preparing_key(&unrelated));
        plan.cursor = plan.count;
        assert!(
            !plan.preparing_key(&early),
            "completed ranges remain evictable"
        );
    }
    #[test]
    fn tier_coverage_counts_unique_matching_frames_without_scanning_the_range() {
        let mut plan = plan(7 * 1024);
        let first = plan.key(0);
        let second = plan.key(1);
        plan.update_retained(&first, true);
        plan.update_retained(&second, true);
        assert_eq!(plan.resident([&first].into_iter()), 2);
        let mut other = first.clone();
        other.channels = fold_platform::desktop::ChannelView::Channel {
            name: "depth.Z".to_string().try_into().unwrap(),
            range: Default::default(),
        };
        plan.update_retained(&other, true);
        assert_eq!(plan.resident(std::iter::empty()), 2);
        plan.update_retained(&first, false);
        assert_eq!(plan.resident(std::iter::empty()), 1);
        plan.limit(1);
        assert_eq!(plan.resident(std::iter::empty()), 0);
    }
    #[test]
    fn seek_edit_port_color_and_resolution_interrupt_preparation() {
        let plan = plan(7 * 1024);
        assert!(plan.compatible(1, &plan.key(1), plan.range));
        assert!(!plan.compatible(2, &plan.key(1), plan.range));
        for key in [
            PreviewKey {
                region: Some(fold_platform::desktop::PreviewRegion {
                    x: 2,
                    y: 2,
                    width: 8,
                    height: 8,
                }),
                ..plan.key(1)
            },
            PreviewKey {
                channels: fold_platform::desktop::ChannelView::Channel {
                    name: "depth.Z".to_owned().try_into().unwrap(),
                    range: fold_platform::desktop::DisplayRange::new(0., 100.).unwrap(),
                },
                ..plan.key(1)
            },
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
