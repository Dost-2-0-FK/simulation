use anyhow::Result;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use crate::{
    domain::{BaseId, MilitaryUnit},
    geometry::{Point, Positioned},
};

use super::parse_id;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedUnit {
    #[serde(rename = "_id")]
    id: ObjectId,
    base_id: String,
    position: Point,
}

impl PersistedUnit {
    pub(super) fn from_unit(unit: &MilitaryUnit) -> Self {
        Self {
            id: ObjectId::new(),
            base_id: unit.base_id().0.to_string(),
            position: unit.position(),
        }
    }

    pub(super) fn into_unit(self) -> Result<MilitaryUnit> {
        let base_id = parse_id::<BaseId>(&self.base_id, "unit base")?;
        Ok(MilitaryUnit::from_persisted(base_id, self.position))
    }
}
