use crate::{
    geometry::{Point, Positioned},
    politics::Zone,
};

/// A [Placement] is associated with a [Zone] and has a position/coordinates.
#[derive(Debug, Clone)]
pub(crate) struct Placement {
    id: String,
    zone: Zone,
    position: Point,
}

crate::impl_positioned!(Placement => position);
