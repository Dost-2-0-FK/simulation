use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::{parse_id, placement_by_id};
use crate::{
    domain::{BaseId, Loot, MilitaryBase, Placement, PlacementId, Target, Trust, TrustId},
    handlers::bases::Financing,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
enum PersistedTarget {
    #[default]
    None,
    Base {
        id: String,
    },
    Trust {
        id: String,
    },
}

impl From<&Target> for PersistedTarget {
    fn from(t: &Target) -> Self {
        match t {
            Target::None => Self::None,
            Target::Base { id, .. } => Self::Base { id: id.0.to_string() },
            Target::Trust { id, .. } => Self::Trust { id: id.0.to_string() },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedBase {
    #[serde(rename = "_id")]
    id: String,
    placement_id: PlacementId,
    financiers: Vec<Financing>,
    #[serde(default = "default_enabled")]
    enabled: bool,
    prioritized: bool,
    #[serde(default)]
    target: PersistedTarget,
    loot: Loot,
}

fn default_enabled() -> bool {
    true
}

impl PersistedBase {
    pub(super) fn from_base(base: &MilitaryBase) -> Self {
        Self {
            id: base.id().0.to_string(),
            placement_id: base.placement_id().clone(),
            financiers: base.financiers().to_vec(),
            enabled: base.enabled(),
            prioritized: base.prioritized(),
            target: base.target().into(),
            loot: base.loot().clone(),
        }
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    /// Create a [MilitaryBase] with `Target::None`. Call [Self::resolve_target] afterwards to set
    /// the actual target once all bases and trusts are loaded.
    pub(super) fn as_base(&self, placements: impl Iterator<Item = Arc<Placement>>) -> Result<MilitaryBase> {
        let id = parse_id::<BaseId>(&self.id, "base")?;
        let placement = placement_by_id(placements, &self.placement_id)?;

        Ok(MilitaryBase::from_persisted(
            id,
            placement,
            self.financiers.clone(),
            self.enabled,
            self.prioritized,
            self.loot.clone(),
        ))
    }

    /// Resolve the persisted target IDs into a [Target] using the already-loaded entity maps.
    /// Returns `None` when the target is [PersistedTarget::None].
    pub(super) fn resolve_target(
        &self,
        bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
        trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    ) -> Result<Option<Target>> {
        match &self.target {
            PersistedTarget::None => Ok(None),
            PersistedTarget::Base { id } => {
                let base_id = parse_id::<BaseId>(id, "target base")?;
                let arc = bases
                    .get(&base_id)
                    .ok_or_else(|| anyhow!("target base {base_id:?} not found"))?;
                Ok(Some(Target::Base {
                    id: base_id,
                    base: arc.clone(),
                }))
            }
            PersistedTarget::Trust { id } => {
                let trust_id = parse_id::<TrustId>(id, "target trust")?;
                let arc = trusts
                    .get(&trust_id)
                    .ok_or_else(|| anyhow!("target trust {trust_id:?} not found"))?;
                Ok(Some(Target::Trust {
                    id: trust_id,
                    trust: arc.clone(),
                }))
            }
        }
    }
}
