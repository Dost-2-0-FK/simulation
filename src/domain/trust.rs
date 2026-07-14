use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::SeqCst},
};

use serde::{Deserialize, Serialize};

use crate::{
    domain::{Loot, LootFactors, Placement, PlacementId},
    geometry::{Point, Positioned},
    handlers::bases::Financing,
    services::credit_exchange_service::{Financiers, Money, Payment, ResourceName, Resources},
};

static TRUST_INSTANCE_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema, PartialEq, Eq, Hash)]
pub(crate) struct TrustId(pub(crate) u64);

/// A [Trust] is built on a placement, and associated with a [Zone]. The association is given
/// implicitly via the [Placement]. Also, the position/coordinates are given via the [Placement].
#[derive(Debug, Clone)]
pub(crate) struct Trust {
    id: TrustId,
    placement: Arc<Placement>,
    financing: Vec<Financing>,
    /// The base value of money to generate in the next production cycle
    income: Money,
    /// The base value of produced resources in the next production cycle
    producing: Resources,
    /// The loot that will be collected if this trust is destroyed.
    loot: Loot,
}

crate::impl_positioned_as_ref!(Trust => placement);

impl Trust {
    /// Create a new trust. Panics if the total count of trusts becomes > [u64::MAX].
    pub(crate) fn new(
        payment: Payment<'_, Self, Financiers>,
        loot_factors: &LootFactors,
        placement: Arc<Placement>,
        resource: ResourceName,
        resource_amount: f32,
        income: Money,
    ) -> Self {
        let id = TRUST_INSTANCE_COUNT.fetch_add(1, SeqCst);
        assert_ne!(id, u64::MAX, "ID counter has overflowed and is no longer unique");
        let loot = Loot::from_cost(payment.cost(), loot_factors);

        Self {
            id: TrustId(id),
            financing: payment.secondary_payers(),
            placement,
            loot,
            income,
            producing: Resources::new_single(resource, resource_amount),
        }
    }

    pub(crate) fn from_persisted(
        id: TrustId,
        placement: Arc<Placement>,
        financing: Vec<Financing>,
        loot: Loot,
        income: Money,
        producing: Resources,
    ) -> Self {
        assert_ne!(id.0, u64::MAX, "ID counter has overflowed and is no longer unique");
        TRUST_INSTANCE_COUNT.fetch_max(id.0 + 1, SeqCst);

        Self {
            id,
            financing,
            placement,
            loot,
            income,
            producing,
        }
    }

    pub(crate) fn id(&self) -> TrustId {
        self.id
    }

    pub(crate) fn placement(&self) -> &Placement {
        &self.placement
    }

    pub(crate) fn placement_id(&self) -> &PlacementId {
        self.placement.id()
    }

    pub(crate) fn financing(&self) -> &[Financing] {
        &self.financing
    }

    pub(crate) fn loot(&self) -> &Loot {
        &self.loot
    }

    pub(crate) fn income(&self) -> Money {
        self.income
    }

    pub(crate) fn producing(&self) -> &Resources {
        &self.producing
    }
}
