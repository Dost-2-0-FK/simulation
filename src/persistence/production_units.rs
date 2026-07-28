use std::sync::Arc;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::domain::{Production, ProductionUnit, ProductionUnitKey, Zone, ZoneKey};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedProductionUnit {
    #[serde(rename = "_id")]
    key: ProductionUnitKey,
    zone: ZoneKey,
    production: Production,
}

impl PersistedProductionUnit {
    pub(super) fn from_production_unit(unit: &ProductionUnit) -> Self {
        Self {
            key: unit.key().clone(),
            zone: unit.zone().key().clone(),
            production: unit.production().clone(),
        }
    }

    pub(super) fn key(&self) -> &ProductionUnitKey {
        &self.key
    }

    pub(super) fn into_production_unit(self, mut zones: impl Iterator<Item = Arc<Zone>>) -> Result<ProductionUnit> {
        let zone = zones.find(|zone| zone.key() == &self.zone).ok_or_else(|| {
            anyhow!(
                "persisted production unit {} references unknown zone {}",
                self.key,
                self.zone
            )
        })?;
        Ok(ProductionUnit::from_persisted(self.key, zone, self.production))
    }
}
