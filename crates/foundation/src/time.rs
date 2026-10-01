use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, fmt};

/// Normalized rational seconds. Denominators are positive; arithmetic never wraps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "TimeRepr", into = "TimeRepr")]
pub struct Time {
    numerator: i64,
    denominator: u32,
}

#[derive(Serialize, Deserialize)]
struct TimeRepr {
    numerator: i64,
    denominator: u32,
}

impl TryFrom<TimeRepr> for Time {
    type Error = TimeError;
    fn try_from(value: TimeRepr) -> Result<Self, Self::Error> {
        Self::new(value.numerator, value.denominator)
    }
}

impl From<Time> for TimeRepr {
    fn from(value: Time) -> Self {
        Self {
            numerator: value.numerator,
            denominator: value.denominator,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeError {
    ZeroDenominator,
    Overflow,
    InvalidRange,
    InvalidRate,
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TimeError {}

#[derive(Clone, Copy, Debug)]
pub enum Rounding {
    Floor,
    Ceil,
    /// Ties are rounded away from zero.
    Nearest,
}

impl Time {
    pub const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };

    pub fn new(numerator: i64, denominator: u32) -> Result<Self, TimeError> {
        Self::normalize(i128::from(numerator), i128::from(denominator))
    }

    fn normalize(numerator: i128, denominator: i128) -> Result<Self, TimeError> {
        if denominator == 0 {
            return Err(TimeError::ZeroDenominator);
        }
        let (mut a, mut b) = (numerator.abs(), denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: (numerator / a)
                .try_into()
                .map_err(|_| TimeError::Overflow)?,
            denominator: (denominator / a)
                .try_into()
                .map_err(|_| TimeError::Overflow)?,
        })
    }

    pub fn numerator(self) -> i64 {
        self.numerator
    }
    pub fn denominator(self) -> u32 {
        self.denominator
    }

    pub fn checked_add(self, other: Self) -> Result<Self, TimeError> {
        Self::normalize(
            i128::from(self.numerator) * i128::from(other.denominator)
                + i128::from(other.numerator) * i128::from(self.denominator),
            i128::from(self.denominator) * i128::from(other.denominator),
        )
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, TimeError> {
        Self::normalize(
            i128::from(self.numerator) * i128::from(other.denominator)
                - i128::from(other.numerator) * i128::from(self.denominator),
            i128::from(self.denominator) * i128::from(other.denominator),
        )
    }

    /// Scale time by an exact rational (e.g. a playback speed).
    pub fn checked_scale(self, numerator: i64, denominator: u32) -> Result<Self, TimeError> {
        Self::normalize(
            i128::from(self.numerator) * i128::from(numerator),
            i128::from(self.denominator) * i128::from(denominator),
        )
    }

    /// Convert to integer sample/frame boundaries at a positive rational rate.
    pub fn to_ticks(
        self,
        rate_numerator: u32,
        rate_denominator: u32,
        rounding: Rounding,
    ) -> Result<i64, TimeError> {
        if rate_numerator == 0 || rate_denominator == 0 {
            return Err(TimeError::InvalidRate);
        }
        let n = i128::from(self.numerator) * i128::from(rate_numerator);
        let d = i128::from(self.denominator) * i128::from(rate_denominator);
        let q = n / d;
        let r = n % d;
        let result = match rounding {
            Rounding::Floor if r < 0 => q - 1,
            Rounding::Ceil if r > 0 => q + 1,
            Rounding::Nearest if r.abs() * 2 >= d => q + n.signum(),
            _ => q,
        };
        result.try_into().map_err(|_| TimeError::Overflow)
    }
}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> Ordering {
        (i128::from(self.numerator) * i128::from(other.denominator))
            .cmp(&(i128::from(other.numerator) * i128::from(self.denominator)))
    }
}
impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Half-open [start, end); empty ranges are valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeRange {
    start: Time,
    end: Time,
}
impl TimeRange {
    pub fn new(start: Time, end: Time) -> Result<Self, TimeError> {
        if end < start {
            return Err(TimeError::InvalidRange);
        }
        Ok(Self { start, end })
    }
    pub fn start(self) -> Time {
        self.start
    }
    pub fn end(self) -> Time {
        self.end
    }
    pub fn contains(self, time: Time) -> bool {
        self.start <= time && time < self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_fractional_frames_and_samples() {
        let frame = Time::new(1001, 24000).unwrap();
        let second = frame.checked_scale(24000, 1001).unwrap();
        assert_eq!(second, Time::new(1, 1).unwrap());
        assert_eq!(frame.to_ticks(24000, 1001, Rounding::Floor), Ok(1));
        assert_eq!(frame.to_ticks(48000, 1, Rounding::Floor), Ok(2002));
        assert_eq!(
            frame.checked_add(frame).unwrap().checked_sub(frame),
            Ok(frame)
        );
        assert_eq!(Time::new(0, 77), Ok(Time::ZERO));
        assert_eq!(Time::new(2, 4), Time::new(1, 2));
    }

    #[test]
    fn rounding_and_half_open_boundaries() {
        let half = Time::new(-1, 2).unwrap();
        assert_eq!(half.to_ticks(1, 1, Rounding::Floor), Ok(-1));
        assert_eq!(half.to_ticks(1, 1, Rounding::Ceil), Ok(0));
        assert_eq!(half.to_ticks(1, 1, Rounding::Nearest), Ok(-1));
        let one = Time::new(1, 1).unwrap();
        assert_eq!(
            Time::new(1, 2).unwrap().to_ticks(1, 1, Rounding::Nearest),
            Ok(1)
        );
        let left = TimeRange::new(Time::ZERO, one).unwrap();
        let right = TimeRange::new(one, Time::new(2, 1).unwrap()).unwrap();
        assert!(!left.contains(one));
        assert!(right.contains(one));
        assert!(!TimeRange::new(one, one).unwrap().contains(one));
        assert_eq!(
            TimeRange::new(one, Time::ZERO),
            Err(TimeError::InvalidRange)
        );
    }

    #[test]
    fn invalid_and_overflowing_time_is_rejected() {
        assert_eq!(Time::new(1, 0), Err(TimeError::ZeroDenominator));
        let max = Time::new(i64::MAX, 1).unwrap();
        assert_eq!(max.checked_add(max), Err(TimeError::Overflow));
        assert_eq!(max.checked_scale(2, 1), Err(TimeError::Overflow));
        assert_eq!(
            max.to_ticks(2, 1, Rounding::Floor),
            Err(TimeError::Overflow)
        );
        assert_eq!(
            max.to_ticks(0, 1, Rounding::Floor),
            Err(TimeError::InvalidRate)
        );
        assert_eq!(max.checked_scale(1, 0), Err(TimeError::ZeroDenominator));
        let min = Time::new(i64::MIN, 1).unwrap();
        assert_eq!(min.checked_sub(min), Ok(Time::ZERO));
        assert_eq!(min.checked_scale(-1, 1), Err(TimeError::Overflow));
        assert_eq!(
            Time::new(1, u32::MAX).unwrap().checked_scale(1, 2),
            Err(TimeError::Overflow)
        );
    }
}
