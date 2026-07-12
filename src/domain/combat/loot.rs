use serde::Serialize;

use crate::services::credit_exchange_service::{Money, Resources};

#[derive(Debug, Default, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct Loot {
    money: Money,
    resources: Resources,
}
