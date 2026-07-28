use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::SeqCst},
};

use serde::{Deserialize, Serialize};

use crate::{
    domain::{Loot, LootFactors, Placement, PlacementId, Production},
    geometry::{Point, Positioned},
    handlers::bases::Financing,
    services::credit_exchange_service::{
        Cost, Financiers, Money, Payment, ResourceName, ResourceValue, Resources, Share,
    },
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
    production: Production,
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
        let loot = Loot::from_cost(payment.cost(), loot_factors);
        Self::new_prepaid_with_loot(
            payment.secondary_payers(),
            placement,
            resource,
            resource_amount,
            income,
            loot,
        )
    }

    /// Create a trust whose configured financing has already been paid.
    pub(crate) fn new_prepaid(
        financing: Vec<Financing>,
        cost: &Cost<Self>,
        loot_factors: &LootFactors,
        placement: Arc<Placement>,
        resource: ResourceName,
        resource_amount: f32,
        income: Money,
    ) -> Self {
        let loot = Loot::from_cost(cost, loot_factors);
        Self::new_prepaid_with_loot(financing, placement, resource, resource_amount, income, loot)
    }

    fn new_prepaid_with_loot(
        financing: Vec<Financing>,
        placement: Arc<Placement>,
        resource: ResourceName,
        resource_amount: f32,
        income: Money,
        loot: Loot,
    ) -> Self {
        let id = TRUST_INSTANCE_COUNT.fetch_add(1, SeqCst);
        assert_ne!(id, u64::MAX, "ID counter has overflowed and is no longer unique");

        Self {
            id: TrustId(id),
            financing,
            placement,
            loot,
            production: Production::new(resource, resource_amount, income),
        }
    }

    pub(crate) fn from_persisted(
        id: TrustId,
        placement: Arc<Placement>,
        financing: Vec<Financing>,
        loot: Loot,
        production: Production,
    ) -> Self {
        assert_ne!(id.0, u64::MAX, "ID counter has overflowed and is no longer unique");
        TRUST_INSTANCE_COUNT.fetch_max(id.0 + 1, SeqCst);

        Self {
            id,
            financing,
            placement,
            loot,
            production,
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

    pub(crate) fn base_income(&self) -> Money {
        self.production.base_income()
    }

    pub(crate) fn resource_name(&self) -> &ResourceName {
        self.production.resource_name()
    }

    pub(crate) fn producing_base_value(&self) -> &Resources {
        self.production.producing_base_value()
    }

    pub(crate) async fn production_with_inhibition(&self, factor: Share) -> Resources {
        self.production.with_factor(
            self.placement
                .zone()
                .trust_production_factor()
                .await
                .combined_with(factor),
        )
    }

    pub(crate) fn income(&self, produced: ResourceValue<'_>, existing_resource_units: f32) -> Money {
        self.production.income(produced, existing_resource_units)
    }
}
