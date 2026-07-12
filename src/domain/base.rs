use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::SeqCst},
};

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    domain::{Bloc, BlocName, Loot, Placement, PlacementId, Trust, TrustId},
    geometry::{Point, Positioned},
    handlers::bases::Financing,
    services::credit_exchange_service::{Financiers, Payment},
};

static BASE_INSTANCE_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema, PartialEq, Eq, Hash, PartialOrd, Ord)]
// TODO make this inner field private (just for now it's not to enable a shortcut to create a unit)
pub(crate) struct BaseId(pub(crate) u64);

/// Which entity a base's units will move toward as their secondary target. The variants store the
/// designated target alongside a cached copy of its ID so callers can read the ID without
/// acquiring a lock.
///
/// Regardless of the variant, enemy units that are closer than the secondary target are always
/// prioritised. `None` means units only ever chase the nearest enemy unit.
#[derive(Debug, Clone, Default)]
pub(crate) enum Target {
    #[default]
    None,
    Base {
        id: BaseId,
        base: Arc<RwLock<MilitaryBase>>,
    },
    Trust {
        id: TrustId,
        trust: Arc<RwLock<Trust>>,
    },
}

/// A [MilitaryBase] is built on a placement, and associated with a [Zone] and a [Bloc]. The associations are given
/// implicitly via the [Placement], as well as its the position/coordinates.
#[derive(Debug, Clone)]
pub(crate) struct MilitaryBase {
    id: BaseId,
    placement: Arc<Placement>,
    financiers: Vec<Financing>,
    enabled: bool,
    prioritized: bool,
    target: Target,
    /// The loot that will be collected if this base is destroyed.
    loot: Loot,
    /// How much loot has accumulated since the last full hour?
    production_count: Loot,
}

crate::impl_positioned_as_ref!(MilitaryBase => placement);

impl MilitaryBase {
    /// Create a new base. Panics if the total count of bases becomes > [u64::MAX].
    pub(crate) fn new(payment: Payment<'_, Self, Financiers>, placement: Arc<Placement>) -> Self {
        let id = BASE_INSTANCE_COUNT.fetch_add(1, SeqCst);
        assert_ne!(id, u64::MAX, "ID counter has overflowed and is no longer unique");
        let loot = payment.loot().clone();

        Self {
            id: BaseId(id),
            financiers: payment.consume(),
            placement,
            enabled: true,
            prioritized: false,
            target: Target::None,
            loot,
            production_count: Default::default(),
        }
    }

    pub(crate) fn from_persisted(
        id: BaseId,
        placement: Arc<Placement>,
        financiers: Vec<Financing>,
        enabled: bool,
        prioritized: bool,
        loot: Loot,
    ) -> Self {
        assert_ne!(id.0, u64::MAX, "ID counter has overflowed and is no longer unique");
        BASE_INSTANCE_COUNT.fetch_max(id.0 + 1, SeqCst);

        Self {
            id,
            placement,
            financiers,
            enabled,
            prioritized,
            target: Target::None,
            loot,
            production_count: Default::default(),
        }
    }

    pub(crate) fn id(&self) -> BaseId {
        self.id
    }

    pub(crate) fn placement(&self) -> &Placement {
        &self.placement
    }

    pub(crate) fn placement_id(&self) -> &PlacementId {
        self.placement.id()
    }

    pub(crate) fn bloc_name(&self) -> &BlocName {
        self.placement().zone().bloc_name()
    }

    pub(crate) async fn bloc(&self) -> RwLockReadGuard<'_, Bloc> {
        self.placement().zone().bloc().await
    }

    pub(crate) fn financiers(&self) -> &[Financing] {
        &self.financiers
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn prioritized(&self) -> bool {
        self.prioritized
    }

    pub(crate) fn target(&self) -> &Target {
        &self.target
    }

    pub(crate) fn loot(&self) -> &Loot {
        &self.loot
    }

    pub(crate) fn production_count(&self) -> &Loot {
        &self.production_count
    }

    pub(crate) fn clear_production_count(&mut self) -> Loot {
        std::mem::take(&mut self.production_count)
    }

    pub(crate) fn add_production(&mut self, loot: &Loot) {
        self.production_count += loot;
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn set_prioritized(&mut self, prioritized: bool) {
        self.prioritized = prioritized;
    }

    pub(crate) fn set_target(&mut self, target: Target) {
        self.target = target;
    }
}
