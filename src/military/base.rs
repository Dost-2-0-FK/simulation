use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::SeqCst},
};

use serde::{Deserialize, Serialize};

use crate::{
    geometry::{Point, Positioned},
    payment_service::{AdditionalPayer, Money, Payment},
    placement::Placement,
    service::bases::Financing,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
// TODO make this inner field private (just for now it's not to enable a shortcut to create a unit)
pub(crate) struct BaseId(pub(crate) u64);

/// A [MilitaryBase] is built on a placement, and associated with a [Zone] and a [Bloc]. The associations are given
/// implicitly via the [Placement], as well as its the position/coordinates.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct MilitaryBase {
    id: BaseId,
    #[serde(skip)]
    placement: Arc<Placement>,
    financing: Financing,
    /// How much credit has been produced since the last full hour?
    production_count: Money,
}

crate::impl_positioned_as_ref!(MilitaryBase => placement);

impl MilitaryBase {
    /// Create a new base. Panics if the total count of bases becomes > [u64::MAX].
    pub(crate) fn new(payment: Payment<'_, Self, AdditionalPayer>, placement: Arc<Placement>) -> Self {
        /// Count of total base instances
        static INSTANCE_COUNT: AtomicU64 = AtomicU64::new(0);
        let id = INSTANCE_COUNT.fetch_add(1, SeqCst);
        assert_ne!(id, u64::MAX, "ID counter has overflowed and is no longer unique");

        Self {
            id: BaseId(id),
            financing: payment.consume(),
            placement,
            production_count: Default::default(),
        }
    }
}
