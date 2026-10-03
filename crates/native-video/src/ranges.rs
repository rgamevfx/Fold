//! Two bounded codec positions per pinned source. Shared by host telemetry and
//! the helper so reuse/seek witnesses describe the same selection policy.
#[derive(Default)]
pub struct Ranges {
    next: [Option<u32>; 2],
    recent: usize,
}
impl Ranges {
    pub fn is_forward(&self, index: usize, frame: u32) -> bool {
        self.next[index].is_some_and(|next| next <= frame && frame - next <= 32)
    }
    pub fn select(&self, frame: u32) -> usize {
        (0..2)
            .filter(|&index| self.is_forward(index, frame))
            .min_by_key(|&index| frame - self.next[index].unwrap())
            .or_else(|| self.next.iter().position(Option::is_none))
            .unwrap_or(1 - self.recent)
    }
    pub fn complete(&mut self, index: usize, frame: u32) {
        self.next[index] = frame.checked_add(1);
        self.recent = index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternating_positions_reuse_and_a_third_range_evicts_the_oldest() {
        let mut ranges = Ranges::default();
        for (frame, expected, forward) in [
            (0, 0, false),
            (240, 1, false),
            (1, 0, true),
            (241, 1, true),
            (400, 0, false),
            (242, 1, true),
            (2, 0, false),
        ] {
            let index = ranges.select(frame);
            assert_eq!(index, expected);
            assert_eq!(ranges.is_forward(index, frame), forward);
            ranges.complete(index, frame);
        }
    }
}
