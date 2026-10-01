//! Callback-only ring consumption. No allocation, I/O, locks, or media work.
pub(super) struct AudioOutput {
    pub consumer: rtrb::Consumer<(u64, [f32; 2])>,
    pub cursor: u64,
    pub end: u64,
}
impl AudioOutput {
    /// Returns whether this callback emitted underflow silence. Late samples
    /// are discarded; silence advances time so video cannot stall the clock.
    pub fn fill(&mut self, output: &mut [f32], active: bool) -> bool {
        output.fill(0.0);
        if !active {
            return false;
        }
        let mut underrun = false;
        for dst in output.chunks_exact_mut(2) {
            while self
                .consumer
                .peek()
                .is_ok_and(|(sample, _)| *sample < self.cursor)
            {
                let _ = self.consumer.pop();
            }
            if self.cursor < self.end {
                if self
                    .consumer
                    .peek()
                    .is_ok_and(|(sample, _)| *sample == self.cursor)
                {
                    if let Ok((_, frame)) = self.consumer.pop() {
                        dst.copy_from_slice(&frame);
                    }
                } else {
                    underrun = true;
                }
            }
            self.cursor = self.cursor.saturating_add(1);
        }
        underrun
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn underflow_advances_and_discards_late_samples() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(8);
        let mut callback = AudioOutput {
            consumer,
            cursor: 0,
            end: 4,
        };
        let mut buffer = [9.0; 4];
        assert!(callback.fill(&mut buffer, true));
        assert_eq!(buffer, [0.0; 4]);
        assert_eq!(callback.cursor, 2);
        producer.push((0, [1.0; 2])).unwrap(); // missed its deadline
        producer.push((2, [0.5; 2])).unwrap();
        producer.push((3, [0.25; 2])).unwrap();
        assert!(!callback.fill(&mut buffer, true));
        assert_eq!(buffer, [0.5, 0.5, 0.25, 0.25]);
        assert!(!callback.fill(&mut buffer, true)); // after EOF is not underflow
        assert_eq!(buffer, [0.0; 4]);
    }
    #[test]
    fn inactive_generation_is_silent_without_consuming() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(8);
        producer.push((12, [1.0; 2])).unwrap();
        let mut callback = AudioOutput {
            consumer,
            cursor: 12,
            end: 13,
        };
        let mut buffer = [9.0; 2];
        assert!(!callback.fill(&mut buffer, false));
        assert_eq!(buffer, [0.0; 2]);
        assert_eq!(callback.cursor, 12);
        assert!(!callback.fill(&mut buffer, true));
        assert_eq!(buffer, [1.0; 2]);
    }
}
