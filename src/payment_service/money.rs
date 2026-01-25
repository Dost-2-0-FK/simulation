use serde::Deserialize;

#[derive(Debug, Default, Copy, Clone, Deserialize)]
pub(crate) struct Money(f32);
