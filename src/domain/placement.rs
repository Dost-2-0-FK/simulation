use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    domain::Zone,
    geometry::{Point, Positioned},
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct PlacementId(String);

/// A [Placement] is associated with a [Zone] and has a position/coordinates.
#[derive(Debug, Clone)]
pub(crate) struct Placement {
    id: PlacementId,
    zone: Arc<Zone>,
    position: Point,
}

impl Placement {
    pub(crate) fn new(id: PlacementId, zone: Arc<Zone>, position: Point) -> Self {
        Self { id, zone, position }
    }

    pub(crate) fn id(&self) -> &PlacementId {
        &self.id
    }

    pub(crate) fn zone(&self) -> &Zone {
        &self.zone
    }
}

crate::impl_positioned!(Placement => position);
