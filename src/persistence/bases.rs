use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{parse_id, placement_by_id};
use crate::{
    domain::{BaseId, MilitaryBase, Placement, PlacementId, Target},
    handlers::bases::Financing,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedBase {
    #[serde(rename = "_id")]
    id: String,
    placement_id: PlacementId,
    financiers: Vec<Financing>,
    prioritized: bool,
    target: Target,
}

impl PersistedBase {
    pub(super) fn from_base(base: &MilitaryBase) -> Self {
        Self {
            id: base.id().0.to_string(),
            placement_id: base.placement_id().clone(),
            financiers: base.financiers().to_vec(),
            prioritized: base.prioritized(),
            target: base.target(),
        }
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn into_base(self, placements: impl Iterator<Item = Arc<Placement>>) -> Result<MilitaryBase> {
        let id = parse_id::<BaseId>(&self.id, "base")?;
        let placement = placement_by_id(placements, &self.placement_id)?;

        Ok(MilitaryBase::from_persisted(
            id,
            placement,
            self.financiers,
            self.prioritized,
            self.target,
        ))
    }
}
