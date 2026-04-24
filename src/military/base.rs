use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::SeqCst},
};

use serde::{Deserialize, Serialize};

use crate::{
    geometry::{Point, Positioned},
    payment_service::{Financiers, Money, Payment},
    placement::Placement,
    service::bases::Financing,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
// TODO make this inner field private (just for now it's not to enable a shortcut to create a unit)
pub(crate) struct BaseId(pub(crate) u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Target {
    Trust,
    Base,
    Unit,
}

/// A [MilitaryBase] is built on a placement, and associated with a [Zone] and a [Bloc]. The associations are given
/// implicitly via the [Placement], as well as its the position/coordinates.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct MilitaryBase {
    id: BaseId,
    #[serde(skip)]
    placement: Arc<Placement>,
    financiers: Vec<Financing>,
    prioritized: bool,
    target: Target,
    /// How much credit has been produced since the last full hour?
    production_count: Money,
}

crate::impl_positioned_as_ref!(MilitaryBase => placement);

impl MilitaryBase {
    /// Create a new base. Panics if the total count of bases becomes > [u64::MAX].
    pub(crate) fn new(payment: Payment<'_, Self, Financiers>, placement: Arc<Placement>) -> Self {
        /// Count of total base instances
        static INSTANCE_COUNT: AtomicU64 = AtomicU64::new(0);
        let id = INSTANCE_COUNT.fetch_add(1, SeqCst);
        assert_ne!(id, u64::MAX, "ID counter has overflowed and is no longer unique");

        Self {
            id: BaseId(id),
            financiers: payment.consume(),
            placement,
            prioritized: false,
            target: Target::Trust,
            production_count: Default::default(),
        }
    }

    pub(crate) fn id(&self) -> BaseId {
        self.id
    }

    pub(crate) fn placement(&self) -> &Placement {
        &self.placement
    }

    pub(crate) fn financiers(&self) -> &[Financing] {
        &self.financiers
    }

    pub(crate) fn prioritized(&self) -> bool {
        self.prioritized
    }

    pub(crate) fn target(&self) -> Target {
        self.target
    }

    pub(crate) fn set_prioritized(&mut self, prioritized: bool) {
        self.prioritized = prioritized;
    }

    pub(crate) fn set_target(&mut self, target: Target) {
        self.target = target;
    }
}
