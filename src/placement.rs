use std::sync::Arc;

use serde::Deserialize;

use crate::{
    geometry::{Point, Positioned},
    politics::Zone,
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
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

    #[expect(dead_code)]
    pub(crate) fn zone(&self) -> &Zone {
        &self.zone
    }
}

crate::impl_positioned!(Placement => position);
