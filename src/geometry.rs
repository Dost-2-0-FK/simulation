use std::ops::{Add, Mul, Sub};

use ordered_float::NotNan;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, utoipa::ToSchema, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub(crate) struct Point {
    #[schema(value_type = f64)]
    x: NotNan<f64>,
    #[schema(value_type = f64)]
    y: NotNan<f64>,
}

impl Point {
    pub(crate) fn new(x: NotNan<f64>, y: NotNan<f64>) -> Self {
        Self { x, y }
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<NotNan<f64>> for Point {
    type Output = Self;
    fn mul(self, scale: NotNan<f64>) -> Self {
        Self::new(self.x * scale, self.y * scale)
    }
}

impl Positioned for Point {
    fn position(&self) -> Point {
        *self
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, PartialOrd)]
pub(crate) struct Distance(f64);

impl std::ops::Div for Distance {
    type Output = f64;
    fn div(self, rhs: Self) -> f64 {
        self.0 / rhs.0
    }
}

impl PartialEq<f64> for Distance {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<f64> for Distance {
    fn partial_cmp(&self, other: &f64) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}
pub(crate) trait Positioned {
    fn position(&self) -> Point;

    fn distance_to(&self, other: &impl Positioned) -> Distance
    where
        Self: Sized,
    {
        let a = self.position();
        let b = other.position();
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        Distance((dx * dx + dy * dy).sqrt())
    }
}

/// Generate a pass-through [Positioned] implementation for a struct which has a field that implements [Positioned].
#[macro_export]
macro_rules! impl_positioned {
    ($struct:path => $field:ident) => {
        impl Positioned for $struct {
            fn position(&self) -> Point {
                Positioned::position(&self.$field)
            }
        }
    };
}

/// Generate a pass-through [Positioned] implementation for a struct which has a field which is AsRef<T> where T
/// implements [Positioned].
#[macro_export]
macro_rules! impl_positioned_as_ref {
    ($struct:path => $field:ident) => {
        impl Positioned for $struct {
            fn position(&self) -> Point {
                Positioned::position(self.$field.as_ref())
            }
        }
    };
}
