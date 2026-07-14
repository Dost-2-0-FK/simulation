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
    #[serde(default)]
    production_count: Loot,
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
            production_count: base.production_count().clone(),
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
            self.production_count.clone(),
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

#[cfg(test)]
mod tests {
    use mongodb::bson::{Bson, Document, doc, from_bson, from_document, to_document};

    use super::*;

    fn placement_id() -> PlacementId {
        from_bson(Bson::String("placement-1".to_string())).expect("placement id deserializes")
    }

    fn loot(money: f32, iron: f32) -> Loot {
        from_document(doc! {
            "money": money,
            "resources": {
                "iron": iron,
            },
        })
        .expect("loot deserializes")
    }

    #[test]
    fn serializes_production_count_and_target() {
        let persisted = PersistedBase {
            id: "1".to_string(),
            placement_id: placement_id(),
            financiers: Vec::new(),
            enabled: true,
            prioritized: false,
            target: PersistedTarget::Base { id: "2".to_string() },
            loot: loot(3.0, 4.0),
            production_count: loot(7.0, 11.0),
        };

        let document = to_document(&persisted).expect("base serializes");

        assert!(document.contains_key("production_count"));
        assert_eq!(
            document
                .get_document("target")
                .expect("target is a document")
                .get_str("type")
                .expect("target has type"),
            "base"
        );
        assert_eq!(
            document
                .get_document("target")
                .expect("target is a document")
                .get_str("id")
                .expect("target has id"),
            "2"
        );

        let decoded: PersistedBase = from_document(document).expect("base deserializes");

        assert_eq!(decoded.production_count.money().value(), 7.0);
        assert_eq!(
            decoded
                .production_count
                .resources()
                .find(|resource| resource.name().as_str() == "iron")
                .expect("iron production is present")
                .value(),
            11.0
        );
        assert!(matches!(decoded.target, PersistedTarget::Base { id } if id == "2"));
    }

    #[test]
    fn defaults_missing_production_count_for_existing_documents() {
        let decoded: PersistedBase = from_document(doc! {
            "_id": "1",
            "placement_id": "placement-1",
            "financiers": Bson::Array(Vec::new()),
            "enabled": true,
            "prioritized": false,
            "target": {
                "type": "trust",
                "id": "3",
            },
            "loot": {
                "money": 3.0,
                "resources": Document::new(),
            },
        })
        .expect("legacy base deserializes");

        assert_eq!(decoded.production_count.money().value(), 0.0);
        assert_eq!(decoded.production_count.resources().count(), 0);
        assert!(matches!(decoded.target, PersistedTarget::Trust { id } if id == "3"));
    }
}
