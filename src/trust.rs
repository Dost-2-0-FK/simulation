use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::SeqCst},
};

use serde::{Deserialize, Serialize};

use crate::{
    geometry::{Point, Positioned},
    payment_service::{AdditionalPayer, Payment},
    placement::Placement,
    service::bases::Financing,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
pub(crate) struct TrustId(pub(crate) u64);

/// A [Trust] is built on a placement, and associated with a [Zone]. The association is given
/// implicitly via the [Placement]. Also, the position/coordinates are given via the [Placement].
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct Trust {
    id: TrustId,
    #[serde(skip)]
    placement: Arc<Placement>,
    financing: Financing,
}

crate::impl_positioned_as_ref!(Trust => placement);

impl Trust {
    /// Create a new trust. Panics if the total count of trusts becomes > [u64::MAX].
    pub(crate) fn new(payment: Payment<'_, Self, AdditionalPayer>, placement: Arc<Placement>) -> Self {
        static INSTANCE_COUNT: AtomicU64 = AtomicU64::new(0);
        let id = INSTANCE_COUNT.fetch_add(1, SeqCst);
        assert_ne!(id, u64::MAX, "ID counter has overflowed and is no longer unique");

        Self {
            id: TrustId(id),
            financing: payment.consume(),
            placement,
        }
    }

    pub(crate) fn id(&self) -> TrustId {
        self.id
    }

    pub(crate) fn placement(&self) -> &Placement {
        &self.placement
    }

    pub(crate) fn financing(&self) -> &Financing {
        &self.financing
    }
}
