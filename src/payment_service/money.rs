use derive_more::Display;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Copy, Clone, Deserialize, Serialize, Display, utoipa::ToSchema)]
pub(crate) struct Money(f32);
