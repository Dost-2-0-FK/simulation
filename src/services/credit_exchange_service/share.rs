use serde::{Deserialize, Serialize};

use super::{Money, Resources};

/// A value between 0 and 1 representing a share of an amount of money or resource, where 1
/// corresponds to the entire amount (i.e., the sum of all shares of a given amount is 1).
#[derive(Debug, Default, Copy, Clone, PartialEq, utoipa::ToSchema, Serialize)]
pub struct Share(f32);

impl Share {
    pub(crate) fn value(self) -> f32 {
        self.0
    }
}

impl From<f32> for Share {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl<'de> Deserialize<'de> for Share {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let share = f32::deserialize(deserializer)?;

        if !(0.0..=1.0).contains(&share) {
            Err(serde::de::Error::custom("share must be between 0 and 1"))
        } else {
            Ok(Share(share))
        }
    }
}

impl std::ops::Mul<Money> for Share {
    type Output = Money;
    fn mul(self, rhs: Money) -> Money {
        rhs * self.0
    }
}

impl std::ops::Mul<Resources> for Share {
    type Output = Resources;
    fn mul(self, rhs: Resources) -> Resources {
        rhs * self.0
    }
}
