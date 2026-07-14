use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{parse_id, placement_by_id};
use crate::{
    domain::{Loot, Placement, PlacementId, Trust, TrustId},
    handlers::bases::Financing,
    services::credit_exchange_service::{Money, Resources},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedTrust {
    #[serde(rename = "_id")]
    id: String,
    placement_id: PlacementId,
    financing: Vec<Financing>,
    loot: Loot,
    income: Money,
    producing: Resources,
}

impl PersistedTrust {
    pub(super) fn from_trust(trust: &Trust) -> Self {
        Self {
            id: trust.id().0.to_string(),
            placement_id: trust.placement_id().clone(),
            financing: trust.financing().to_vec(),
            loot: trust.loot().clone(),
            income: trust.income(),
            producing: trust.producing().clone(),
        }
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn into_trust(self, placements: impl Iterator<Item = Arc<Placement>>) -> Result<Trust> {
        let id = parse_id::<TrustId>(&self.id, "trust")?;
        let placement = placement_by_id(placements, &self.placement_id)?;

        Ok(Trust::from_persisted(
            id,
            placement,
            self.financing,
            self.loot,
            self.income,
            self.producing,
        ))
    }
}
