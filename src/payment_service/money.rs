use derive_more::Display;
use serde::Deserialize;

#[derive(Debug, Default, Copy, Clone, Deserialize, Display)]
pub(crate) struct Money(f32);
