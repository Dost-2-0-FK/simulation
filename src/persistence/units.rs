use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::parse_id;
use crate::{
    domain::{BaseId, MilitaryUnit},
    geometry::{Point, Positioned},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedUnit {
    #[serde(rename = "_id")]
    id: String,
    base_id: String,
    position: Point,
}

impl PersistedUnit {
    pub(super) fn from_unit(unit: &MilitaryUnit) -> Self {
        Self {
            id: unit.id().clone().into(),
            base_id: unit.base_id().0.to_string(),
            position: unit.position(),
        }
    }

    pub(super) fn into_unit(self) -> Result<MilitaryUnit> {
        let base_id = parse_id::<BaseId>(&self.base_id, "unit base")?;
        Ok(MilitaryUnit::from_persisted(self.id, base_id, self.position))
    }
}
