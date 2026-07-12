use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, anyhow};
use mongodb::bson::Uuid;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    domain::{BaseId, Loot, MilitaryBase, MilitaryUnit, UnitState},
    geometry::{Point, Positioned},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedUnit {
    #[serde(rename = "_id")]
    id: Uuid,
    base_id: String,
    position: Point,
    state: UnitState,
    #[serde(default)]
    loot: Loot,
}

impl PersistedUnit {
    pub(super) fn id(&self) -> Uuid {
        self.id
    }

    pub(super) async fn from_unit(unit: &MilitaryUnit) -> Self {
        Self {
            id: unit.id().into(),
            base_id: unit.base().await.id().0.to_string(),
            position: unit.position(),
            state: unit.state(),
            loot: unit.loot().clone(),
        }
    }

    pub(super) fn into_unit(self, bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>) -> Result<MilitaryUnit> {
        let base_id = self
            .base_id
            .parse::<u64>()
            .map(BaseId)
            .map_err(|e| anyhow!("parsing persisted unit base id {}: {e}", self.base_id))?;
        let base = bases
            .get(&base_id)
            .ok_or_else(|| anyhow!("unit {} references unknown base {base_id:?}", self.id))?
            .clone();
        Ok(MilitaryUnit::from_persisted(
            self.id,
            base,
            self.position,
            self.state,
            self.loot,
        ))
    }
}
