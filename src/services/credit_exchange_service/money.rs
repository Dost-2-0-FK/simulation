use derive_more::Display;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Deserialize,
    Serialize,
    Display,
    utoipa::ToSchema,
    PartialOrd,
    PartialEq,
    derive_more::From,
)]
pub(crate) struct Money(f32);

impl Money {
    pub(crate) fn value(self) -> f32 {
        self.0
    }
}

impl std::ops::Sub for Money {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::SubAssign for Money {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl std::ops::AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl std::ops::Mul<f32> for Money {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self(self.0 * rhs)
    }
}

impl std::ops::Div<f32> for Money {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self(self.0 / rhs)
    }
}
