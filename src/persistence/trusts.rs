use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{parse_id, placement_by_id};
use crate::{
    domain::{Placement, PlacementId, Trust, TrustId},
    handlers::bases::Financing,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedTrust {
    #[serde(rename = "_id")]
    id: String,
    placement_id: PlacementId,
    financing: Vec<Financing>,
}

impl PersistedTrust {
    pub(super) fn from_trust(trust: &Trust) -> Self {
        Self {
            id: trust.id().0.to_string(),
            placement_id: trust.placement_id().clone(),
            financing: trust.financing().to_vec(),
        }
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn into_trust(self, placements: impl Iterator<Item = Arc<Placement>>) -> Result<Trust> {
        let id = parse_id::<TrustId>(&self.id, "trust")?;
        let placement = placement_by_id(placements, &self.placement_id)?;

        Ok(Trust::from_persisted(id, placement, self.financing))
    }
}
