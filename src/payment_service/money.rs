use derive_more::Display;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Copy, Clone, Deserialize, Serialize, Display)]
pub(crate) struct Money(f32);
