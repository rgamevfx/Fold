//! Bounded process-wide admission. Audio and browser preparation have reserved
//! lanes; graphics consumers share weighted, FIFO service at frame boundaries.
pub use fold_media::Cancel;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Viewer,
    Export,
    Prepare,
    Audio,
    Background,
}
const CYCLE: [Class; 6] = [
    Class::Viewer,
    Class::Viewer,
    Class::Viewer,
    Class::Viewer,
    Class::Export,
    Class::Prepare,
];
impl Class {
    fn lane(self) -> usize {
        match self {
            Self::Audio => 1,
            Self::Background => 2,
            _ => 0,
        }
    }
}
#[derive(Default)]
struct State {
    next: u64,
    waiting: Vec<(u64, Class)>,
    active: [bool; 3],
    turn: usize,
    peak: usize,
}
impl State {
    fn selected(&self, lane: usize) -> Option<u64> {
        if lane != 0 {
            return self
                .waiting
                .iter()
                .find(|(_, c)| c.lane() == lane)
                .map(|r| r.0);
        }
        for offset in 0..CYCLE.len() {
            let class = CYCLE[(self.turn + offset) % CYCLE.len()];
            if let Some(&(ticket, _)) = self.waiting.iter().find(|(_, c)| *c == class) {
                return Some(ticket);
            }
        }
        None
    }
    fn admit(&mut self, ticket: u64, class: Class) -> bool {
        let lane = class.lane();
        if self.active[lane] || self.selected(lane) != Some(ticket) {
            return false;
        }
        self.waiting.retain(|r| r.0 != ticket);
        self.active[lane] = true;
        if lane == 0 {
            while CYCLE[self.turn] != class {
                self.turn = (self.turn + 1) % CYCLE.len();
            }
            self.turn = (self.turn + 1) % CYCLE.len();
        }
        true
    }
}
#[derive(Clone, Default)]
pub struct Scheduler(Arc<(Mutex<State>, Condvar)>);
pub struct Permit {
    scheduler: Scheduler,
    lane: usize,
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.scheduler.0.0.lock().unwrap().active[self.lane] = false;
        self.scheduler.0.1.notify_all();
    }
}
impl Scheduler {
    pub fn shared() -> Self {
        static SHARED: OnceLock<Scheduler> = OnceLock::new();
        SHARED.get_or_init(Self::default).clone()
    }
    /// Worker-only, cooperatively cancellable admission. Never use from UI or
    /// audio callbacks. Hold at most one permit and release at each video frame.
    pub fn enter(&self, class: Class, cancel: &Cancel) -> Result<Permit, String> {
        cancel.check()?;
        let (lock, ready) = &*self.0;
        let mut state = lock.lock().unwrap();
        if state.waiting.len() >= 128 {
            return Err("execution queue full (128); retry after completion".into());
        }
        let ticket = state.next;
        state.next = state.next.wrapping_add(1);
        state.waiting.push((ticket, class));
        state.peak = state.peak.max(state.waiting.len());
        loop {
            if let Err(error) = cancel.check() {
                state.waiting.retain(|r| r.0 != ticket);
                ready.notify_all();
                return Err(error);
            }
            if state.admit(ticket, class) {
                return Ok(Permit {
                    scheduler: self.clone(),
                    lane: class.lane(),
                });
            }
            state = ready
                .wait_timeout(state, Duration::from_millis(10))
                .unwrap()
                .0;
        }
    }
    pub fn queue_depth(&self) -> (usize, usize) {
        let state = self.0.0.lock().unwrap();
        (state.waiting.len(), state.peak)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sustained_viewer_pressure_reserves_export_and_range_service() {
        let mut state = State::default();
        for ticket in 0..60 {
            state.waiting.push((ticket, Class::Viewer));
        }
        state
            .waiting
            .extend([(60, Class::Export), (61, Class::Prepare)]);
        let mut order = vec![];
        for _ in 0..6 {
            let ticket = state.selected(0).unwrap();
            let class = state.waiting.iter().find(|r| r.0 == ticket).unwrap().1;
            assert!(state.admit(ticket, class));
            order.push(class);
            state.active[0] = false;
        }
        assert_eq!(
            order,
            [
                Class::Viewer,
                Class::Viewer,
                Class::Viewer,
                Class::Viewer,
                Class::Export,
                Class::Prepare
            ]
        );
    }
    #[test]
    fn audio_and_background_capacity_do_not_wait_for_graphics() {
        let scheduler = Scheduler::default();
        let cancel = Cancel::default();
        let graphics = scheduler.enter(Class::Viewer, &cancel).unwrap();
        let audio = scheduler.enter(Class::Audio, &cancel).unwrap();
        let background = scheduler.enter(Class::Background, &cancel).unwrap();
        assert_eq!(scheduler.queue_depth().0, 0);
        drop((graphics, audio, background));
        assert!(scheduler.enter(Class::Export, &cancel).is_ok());
    }
    #[test]
    fn cancellation_releases_a_waiting_ticket() {
        let scheduler = Scheduler::default();
        let held = scheduler.enter(Class::Viewer, &Cancel::default()).unwrap();
        let cancel = Cancel::default();
        let other = scheduler.clone();
        let token = cancel.clone();
        let worker = std::thread::spawn(move || other.enter(Class::Export, &token).is_err());
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while scheduler.queue_depth().0 == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "ticket was never queued"
            );
            std::thread::yield_now();
        }
        cancel.cancel();
        assert!(worker.join().unwrap());
        assert_eq!(scheduler.queue_depth().0, 0);
        drop(held);
        assert!(scheduler.enter(Class::Export, &Cancel::default()).is_ok());
    }
}
