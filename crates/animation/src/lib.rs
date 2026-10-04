//! Feature-neutral scalar animation. Authoritative times remain rational;
//! floating point is used only for interpolation within a located segment.
use fold_foundation::{ObjectId, Time};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    Hold,
    #[default]
    Linear,
    Bezier,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tangents {
    #[default]
    Auto,
    Linked,
    Broken,
}
/// Handle time is a fraction of its adjacent interval (0..=0.5), value is
/// an offset from the key. Bounded fractions guarantee monotone Bézier time.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Handle {
    pub fraction: f64,
    pub value: f64,
}
impl Default for Handle {
    fn default() -> Self {
        Self {
            fraction: 1. / 3.,
            value: 0.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub id: ObjectId,
    pub time: Time,
    pub value: f64,
    pub interpolation: Interpolation,
    pub tangents: Tangents,
    pub incoming: Handle,
    pub outgoing: Handle,
}
impl Key {
    pub fn new(time: Time, value: f64) -> Self {
        Self {
            id: ObjectId::new(),
            time,
            value,
            interpolation: Interpolation::Linear,
            tangents: Tangents::Auto,
            incoming: Handle::default(),
            outgoing: Handle::default(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub keys: Vec<Key>,
}
pub fn seconds(time: Time) -> f64 {
    time.numerator() as f64 / f64::from(time.denominator())
}
fn interval(a: Time, b: Time) -> f64 {
    // Subtract in the widened rational domain before converting. A sampled time
    // can have a denominator whose difference does not fit the Time contract.
    let numerator = i128::from(b.numerator()) * i128::from(a.denominator())
        - i128::from(a.numerator()) * i128::from(b.denominator());
    let denominator = u64::from(a.denominator()) * u64::from(b.denominator());
    numerator as f64 / denominator as f64
}
impl Curve {
    pub fn validate(&self) -> Result<(), String> {
        if self.keys.is_empty() || self.keys.len() > 18000 {
            return Err("Animation channel requires 1..=18000 keys".into());
        }
        let mut ids = BTreeSet::new();
        for key in &self.keys {
            if !ids.insert(key.id) || !key.value.is_finite() {
                return Err("Animation keys require unique IDs and finite values".into());
            }
            for handle in [key.incoming, key.outgoing] {
                if !(0.0..=0.5).contains(&handle.fraction) || !handle.value.is_finite() {
                    return Err("Invalid animation tangent".into());
                }
            }
        }
        for pair in self.keys.windows(2) {
            if pair[0].time >= pair[1].time || pair[1].time.checked_sub(pair[0].time).is_err() {
                return Err("Animation keys require unique increasing exact times".into());
            }
        }
        if self
            .keys
            .last()
            .unwrap()
            .time
            .checked_sub(self.keys[0].time)
            .is_err()
        {
            return Err("Animation time range is too large".into());
        }
        Ok(())
    }
    /// Insert or update without changing an existing key's identity or tangents.
    pub fn insert(&mut self, time: Time, value: f64) -> ObjectId {
        match self.keys.binary_search_by_key(&time, |k| k.time) {
            Ok(i) => {
                self.keys[i].value = value;
                self.keys[i].id
            }
            Err(i) => {
                let key = Key::new(time, value);
                let id = key.id;
                self.keys.insert(i, key);
                id
            }
        }
    }
    pub fn at(&self, time: Time) -> Option<&Key> {
        self.keys
            .binary_search_by_key(&time, |k| k.time)
            .ok()
            .map(|i| &self.keys[i])
    }
    /// Call on a validated curve. Empty curves have no contribution.
    pub fn sample(&self, time: Time) -> Option<f64> {
        let end = self.keys.partition_point(|k| k.time <= time);
        if end == 0 {
            return self.keys.first().map(|k| k.value);
        }
        if end == self.keys.len() {
            return self.keys.last().map(|k| k.value);
        }
        let a = &self.keys[end - 1];
        if time == a.time {
            return Some(a.value);
        }
        let b = &self.keys[end];
        let ratio = interval(a.time, time) / interval(a.time, b.time);
        Some(match a.interpolation {
            Interpolation::Hold => a.value,
            Interpolation::Linear => a.value * (1. - ratio) + b.value * ratio,
            Interpolation::Bezier => {
                let outgoing = self.handle(end - 1, false);
                let incoming = self.handle(end, true);
                let mut lo = 0.;
                let mut hi = 1.;
                for _ in 0..48 {
                    let t = (lo + hi) * 0.5;
                    if bezier(0., outgoing.fraction, 1. - incoming.fraction, 1., t) < ratio {
                        lo = t;
                    } else {
                        hi = t;
                    }
                }
                bezier(
                    a.value,
                    a.value + outgoing.value,
                    b.value + incoming.value,
                    b.value,
                    (lo + hi) * 0.5,
                )
            }
        })
    }
    pub fn handle(&self, index: usize, incoming: bool) -> Handle {
        let key = &self.keys[index];
        if key.tangents != Tangents::Auto {
            return if incoming { key.incoming } else { key.outgoing };
        }
        let left = index.saturating_sub(1);
        let right = (index + 1).min(self.keys.len() - 1);
        let slope = if left == right {
            0.
        } else {
            let a = &self.keys[left];
            let b = &self.keys[right];
            (b.value - a.value) / interval(a.time, b.time)
        };
        let span = if incoming {
            interval(self.keys[left].time, key.time)
        } else {
            interval(key.time, self.keys[right].time)
        };
        Handle {
            fraction: 1. / 3.,
            value: slope * span / 3. * if incoming { -1. } else { 1. },
        }
    }
    pub fn set_handle(&mut self, index: usize, incoming: bool, handle: Handle) {
        let old_in = self.handle(index, true);
        let old_out = self.handle(index, false);
        let key = &mut self.keys[index];
        if key.tangents == Tangents::Auto {
            key.incoming = old_in;
            key.outgoing = old_out;
            key.tangents = Tangents::Linked;
        }
        if incoming {
            key.incoming = handle;
        } else {
            key.outgoing = handle;
        }
        if key.tangents == Tangents::Linked && index > 0 && index + 1 < self.keys.len() {
            let before = interval(self.keys[index - 1].time, self.keys[index].time);
            let after = interval(self.keys[index].time, self.keys[index + 1].time);
            let key = &mut self.keys[index];
            if incoming {
                key.outgoing = Handle {
                    fraction: handle.fraction,
                    value: -handle.value * after / before,
                };
            } else {
                key.incoming = Handle {
                    fraction: handle.fraction,
                    value: -handle.value * before / after,
                };
            }
        }
    }
}
fn bezier(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    let s = 1. - t;
    a * s * s * s + 3. * b * s * s * t + 3. * c * s * t * t + d * t * t * t
}

#[cfg(test)]
mod tests {
    use super::*;
    fn curve() -> Curve {
        let mut c = Curve::default();
        c.insert(Time::ZERO, 0.);
        c.insert(Time::new(1, 1).unwrap(), 10.);
        c
    }
    #[test]
    fn exact_insert_updates_identity_and_components_are_independent() {
        let mut x = curve();
        let y = x.clone();
        let time = Time::new(1001, 24000).unwrap();
        let id = x.insert(time, 3.);
        assert_eq!(x.insert(time, 4.), id);
        assert_eq!(x.keys.len(), 3);
        assert_eq!(x.sample(time), Some(4.));
        assert_eq!(y.keys.len(), 2);
        x.validate().unwrap();
    }
    #[test]
    fn hold_linear_and_bezier_are_random_access_and_clamped_at_endpoints() {
        let mut c = curve();
        assert_eq!(c.sample(Time::new(-1, 1).unwrap()), Some(0.));
        assert_eq!(c.sample(Time::new(2, 1).unwrap()), Some(10.));
        assert_eq!(c.sample(Time::new(1, 4).unwrap()), Some(2.5));
        c.keys[0].interpolation = Interpolation::Hold;
        assert_eq!(c.sample(Time::new(3, 4).unwrap()), Some(0.));
        assert_eq!(c.sample(Time::new(1, 1).unwrap()), Some(10.));
        c.keys[0].interpolation = Interpolation::Bezier;
        assert_eq!(c.sample(Time::ZERO), Some(0.));
        for key in &mut c.keys {
            key.tangents = Tangents::Broken;
        }
        for (n, expected) in [(3, 8.4375), (1, 1.5625), (2, 5.), (1, 1.5625)] {
            assert!((c.sample(Time::new(n, 4).unwrap()).unwrap() - expected).abs() < 1e-10);
        }
    }
    #[test]
    fn weighted_handles_invert_time_and_link_slopes_across_unequal_intervals() {
        let mut c = curve();
        c.insert(Time::new(3, 1).unwrap(), 20.);
        c.keys[1].tangents = Tangents::Linked;
        c.set_handle(
            1,
            true,
            Handle {
                fraction: 0.25,
                value: -2.,
            },
        );
        assert_eq!(
            c.keys[1].outgoing,
            Handle {
                fraction: 0.25,
                value: 4.
            }
        );
        c.keys[0].interpolation = Interpolation::Bezier;
        c.keys[0].tangents = Tangents::Broken;
        c.keys[0].outgoing = Handle {
            fraction: 0.1,
            value: 1.,
        };
        c.keys[1].incoming = Handle {
            fraction: 0.4,
            value: -4.,
        };
        for n in 1..10 {
            assert!((c.sample(Time::new(n, 10).unwrap()).unwrap() - n as f64).abs() < 1e-9);
        }
    }
    #[test]
    fn rejects_collisions_invalid_handles_and_nonfinite_values() {
        let mut c = curve();
        c.keys[1].time = Time::ZERO;
        assert!(c.validate().is_err());
        let mut c = curve();
        c.keys[1].outgoing.fraction = 0.6;
        assert!(c.validate().is_err());
        let mut c = curve();
        c.keys[0].value = f64::NAN;
        assert!(c.validate().is_err());
        let mut c = curve();
        c.keys[1].id = c.keys[0].id;
        assert!(c.validate().is_err());
    }
    #[test]
    fn sample_accepts_a_query_whose_difference_exceeds_time_denominator_capacity() {
        let mut c = Curve::default();
        c.insert(Time::new(1, 65537).unwrap(), 0.);
        c.insert(Time::new(2, 65537).unwrap(), 1.);
        c.validate().unwrap();
        assert!(c.sample(Time::new(1, 65521).unwrap()).unwrap().is_finite());
    }
}
