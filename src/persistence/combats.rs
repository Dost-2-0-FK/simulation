use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, anyhow};
use mongodb::bson::Uuid;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    domain::{
        BaseId, BlocKey, Combat, CombatEvent, CombatState, CombatStructureParameters, CombatStructureSnapshot,
        MilitaryBase, MilitaryUnit, Trust, TrustId, UnitId,
    },
    geometry::Point,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct PersistedCombatUnits {
    bloc: BlocKey,
    unit_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum PersistedCombatStructure {
    None,
    Trust { id: String, destruction_threshold: u32 },
    Base { id: String, destruction_threshold: u32 },
}

impl PersistedCombatStructure {
    async fn from_combat(combat: &Combat) -> Self {
        match combat.structure_snapshot().await {
            CombatStructureSnapshot::None => Self::None,
            CombatStructureSnapshot::Trust {
                id,
                destruction_threshold,
            } => Self::Trust {
                id: id.0.to_string(),
                destruction_threshold,
            },
            CombatStructureSnapshot::Base {
                id,
                destruction_threshold,
            } => Self::Base {
                id: id.0.to_string(),
                destruction_threshold,
            },
        }
    }

    fn into_structure(
        self,
        bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
        trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    ) -> Result<CombatStructureParameters> {
        match self {
            Self::None => Ok(CombatStructureParameters::None),
            Self::Trust {
                id,
                destruction_threshold,
            } => {
                let trust_id = id
                    .parse::<u64>()
                    .map(TrustId)
                    .map_err(|e| anyhow!("parsing persisted combat trust id {id}: {e}"))?;
                let trust = trusts
                    .get(&trust_id)
                    .ok_or_else(|| anyhow!("combat references unknown trust {trust_id:?}"))?;
                Ok(CombatStructureParameters::Trust(trust.clone(), destruction_threshold))
            }
            Self::Base {
                id,
                destruction_threshold,
            } => {
                let base_id = id
                    .parse::<u64>()
                    .map(BaseId)
                    .map_err(|e| anyhow!("parsing persisted combat base id {id}: {e}"))?;
                let base = bases
                    .get(&base_id)
                    .ok_or_else(|| anyhow!("combat references unknown base {base_id:?}"))?;
                Ok(CombatStructureParameters::Base(base.clone(), destruction_threshold))
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedCombat {
    #[serde(rename = "_id")]
    id: Uuid,
    position: Point,
    units: Vec<PersistedCombatUnits>,
    structure: PersistedCombatStructure,
    #[serde(default)]
    state: CombatState,
    #[serde(default)]
    events: Vec<CombatEvent>,
}

impl PersistedCombat {
    pub(super) async fn from_combat(combat: &Combat) -> Self {
        let units = combat
            .unit_ids_by_bloc()
            .await
            .into_iter()
            .map(|(bloc, unit_ids)| PersistedCombatUnits {
                bloc,
                unit_ids: unit_ids.into_iter().map(Into::into).collect(),
            })
            .collect();

        Self {
            id: combat.id().into(),
            position: combat.position(),
            units,
            structure: PersistedCombatStructure::from_combat(combat).await,
            state: combat.state(),
            events: combat.events().to_vec(),
        }
    }

    pub(super) fn id(&self) -> Uuid {
        self.id
    }

    pub(super) fn into_combat(
        self,
        units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
        bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
        trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    ) -> Result<Combat> {
        let mut units_by_bloc = HashMap::with_capacity(self.units.len());
        for group in self.units {
            let mut combat_units = HashMap::with_capacity(group.unit_ids.len());
            for unit_id in group.unit_ids {
                let unit_id = UnitId::from(unit_id);
                let unit = units
                    .get(&unit_id)
                    .ok_or_else(|| anyhow!("combat {} references unknown unit {unit_id:?}", self.id))?;
                combat_units.insert(unit_id, unit.clone());
            }
            units_by_bloc.insert(group.bloc, combat_units);
        }

        Ok(Combat::from_persisted(
            self.id,
            self.position,
            units_by_bloc,
            self.structure.into_structure(bases, trusts)?,
            self.state,
            self.events,
        ))
    }
}
