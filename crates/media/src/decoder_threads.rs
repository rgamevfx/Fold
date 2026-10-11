//! Extra codec threads are leased for the lifetime of a retained process.
//! Reserve a baseline thread for each of the six regular media process slots:
//! an idle retained decoder cannot starve another decoder's first thread.
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) struct Threads {
    pool: Arc<Mutex<usize>>,
    extra: usize,
}
impl Threads {
    pub fn acquire() -> Self {
        static POOL: OnceLock<Arc<Mutex<usize>>> = OnceLock::new();
        let pool = POOL.get_or_init(|| {
            let cpus = std::thread::available_parallelism().map_or(1, usize::from);
            Arc::new(Mutex::new(cpus.saturating_sub(6)))
        });
        Self::from_pool(pool.clone())
    }
    fn from_pool(pool: Arc<Mutex<usize>>) -> Self {
        let extra = {
            let mut free = pool.lock().unwrap();
            let extra = (*free).min(7);
            *free -= extra;
            extra
        };
        Self { pool, extra }
    }
    pub fn count(&self) -> usize {
        self.extra + 1
    }
}
impl Drop for Threads {
    fn drop(&mut self) {
        *self.pool.lock().unwrap() += self.extra;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_decoders_share_extras_without_starving_baseline() {
        let pool = Arc::new(Mutex::new(10));
        let a = Threads::from_pool(pool.clone());
        let b = Threads::from_pool(pool.clone());
        let c = Threads::from_pool(pool.clone());
        assert_eq!((a.count(), b.count(), c.count()), (8, 4, 1));
        drop(a);
        let d = Threads::from_pool(pool.clone());
        assert_eq!(d.count(), 8);
        drop((b, c, d));
        assert_eq!(*pool.lock().unwrap(), 10);
    }
}
