use std::sync::Arc;

use tokio::sync::RwLock;

use crate::domain::{BlocName, MilitaryBase, Trust};

/// Whether a structure exists at a combat's position. Note: this may change when a structure is destroyed.
pub(super) enum CombatStructure {
    /// No structure at the combat's position
    None,
    /// A trust exists at the combat's position
    Trust {
        trust: Arc<RwLock<Trust>>,
        /// Count of many enemy units required for destruction
        destruction_threshold: u32,
    },
    /// A base exists at the combat's position
    Base {
        base: Arc<RwLock<MilitaryBase>>,
        /// Count of many enemy units required for destruction
        destruction_threshold: u32,
    },
}

impl CombatStructure {
    pub(super) fn destruction_threshold(&self) -> Option<u32> {
        if let CombatStructure::Trust {
            destruction_threshold, ..
        }
        | CombatStructure::Base {
            destruction_threshold, ..
        } = self
        {
            return Some(*destruction_threshold);
        }
        None
    }

    pub(super) async fn bloc(&self) -> Option<BlocName> {
        match self {
            CombatStructure::None => None,
            CombatStructure::Trust { trust, .. } => {
                let trust = trust.read().await;
                Some(trust.placement().zone().bloc().name().clone())
            }

            CombatStructure::Base { base, .. } => {
                let trust = base.read().await;
                Some(trust.placement().zone().bloc().name().clone())
            }
        }
    }
}
