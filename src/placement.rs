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
    zone: Zone,
    position: Point,
}

crate::impl_positioned!(Placement => position);
