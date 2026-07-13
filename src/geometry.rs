use std::ops::{Add, Mul, Sub};

use ordered_float::NotNan;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, utoipa::ToSchema, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub(crate) struct Point {
    #[schema(value_type = f64)]
    x: NotNan<f64>,
    #[schema(value_type = f64)]
    y: NotNan<f64>,
}

impl Point {
    pub(crate) fn new(x: NotNan<f64>, y: NotNan<f64>) -> Self {
        Self { x, y }
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<NotNan<f64>> for Point {
    type Output = Self;
    fn mul(self, scale: NotNan<f64>) -> Self {
        Self::new(self.x * scale, self.y * scale)
    }
}

impl Positioned for Point {
    fn position(&self) -> Point {
        *self
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, PartialOrd)]
pub(crate) struct Distance(f64);

impl Distance {
    /// Builds a straight-line distance from already resolved axis deltas.
    fn from_components(dx: NotNan<f64>, dy: NotNan<f64>) -> Self {
        Distance((dx * dx + dy * dy).sqrt())
    }
}

impl std::ops::Div for Distance {
    type Output = f64;
    fn div(self, rhs: Self) -> f64 {
        self.0 / rhs.0
    }
}

impl PartialEq<f64> for Distance {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

impl PartialOrd<f64> for Distance {
    fn partial_cmp(&self, other: &f64) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
/// Half-open world bounds that wrap on both axes.
///
/// Coordinates are canonical inside `[min_x, max_x)` and `[min_y, max_y)`. Moving past one edge re-enters at the
/// opposite edge.
pub(crate) struct WorldBounds {
    min_x: NotNan<f64>,
    max_x: NotNan<f64>,
    min_y: NotNan<f64>,
    max_y: NotNan<f64>,
}

impl WorldBounds {
    /// Verifies that each configured axis has a non-empty half-open span.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.min_x >= self.max_x {
            return Err("world min_x must be less than max_x");
        }
        if self.min_y >= self.max_y {
            return Err("world min_y must be less than max_y");
        }
        Ok(())
    }

    /// Returns the shortest wraparound distance between two positioned values.
    ///
    /// The result uses the shorter delta on each axis, so points near opposite horizontal or vertical edges can be
    /// closer through the wrapped edge than across the displayed rectangle.
    pub(crate) fn distance_between(&self, a: impl Positioned, b: impl Positioned) -> Distance {
        let delta = self.shortest_delta(a.position(), b.position());
        Distance::from_components(delta.x, delta.y)
    }

    /// Advances `from` by at most `step` toward `target` along the shortest wraparound path.
    ///
    /// If `step` reaches or passes the target, the returned point is the canonical wrapped target. Otherwise the
    /// intermediate point is wrapped back into the configured half-open bounds.
    pub(crate) fn step_toward(&self, from: Point, target: Point, step: Distance) -> Point {
        let target = self.wrap(target);
        let diff = self.shortest_delta(from, target);
        let dist = Distance::from_components(diff.x, diff.y);
        if dist <= step {
            return target;
        }

        let scale = NotNan::new(step / dist).expect("We return early above in the case `dist` is 0");
        self.wrap(from + diff * scale)
    }

    /// Canonicalizes a point into the configured half-open world bounds.
    pub(crate) fn wrap(&self, point: Point) -> Point {
        Point::new(
            wrap_coordinate(point.x, self.min_x, self.max_x),
            wrap_coordinate(point.y, self.min_y, self.max_y),
        )
    }

    /// Returns the shortest signed axis deltas from `from` to `to` in this wrapped world.
    fn shortest_delta(&self, from: Point, to: Point) -> Point {
        let from = self.wrap(from);
        let to = self.wrap(to);
        Point::new(
            shortest_axis_delta(from.x, to.x, self.width()),
            shortest_axis_delta(from.y, to.y, self.height()),
        )
    }

    /// Returns the horizontal wrap span.
    fn width(&self) -> NotNan<f64> {
        self.max_x - self.min_x
    }

    /// Returns the vertical wrap span.
    fn height(&self) -> NotNan<f64> {
        self.max_y - self.min_y
    }
}

/// Wraps one coordinate into the half-open interval `[min, max)`.
fn wrap_coordinate(value: NotNan<f64>, min: NotNan<f64>, max: NotNan<f64>) -> NotNan<f64> {
    let span = *max - *min;
    NotNan::new((*value - *min).rem_euclid(span) + *min).expect("finite coordinates remain finite")
}

/// Returns the shortest signed delta from `from` to `to` across a wrapping axis.
fn shortest_axis_delta(from: NotNan<f64>, to: NotNan<f64>, span: NotNan<f64>) -> NotNan<f64> {
    let forward = (*to - *from).rem_euclid(*span);
    let half_span = *span / 2.0;
    let delta = if forward > half_span { forward - *span } else { forward };
    NotNan::new(delta).expect("finite coordinates remain finite")
}

pub(crate) trait Positioned {
    fn position(&self) -> Point;
}

/// Generate a pass-through [Positioned] implementation for a struct which has a field that implements [Positioned].
#[macro_export]
macro_rules! impl_positioned {
    ($struct:path => $field:ident) => {
        impl Positioned for $struct {
            fn position(&self) -> Point {
                Positioned::position(&self.$field)
            }
        }
    };
}

/// Generate a pass-through [Positioned] implementation for a struct which has a field which is AsRef<T> where T
/// implements [Positioned].
#[macro_export]
macro_rules! impl_positioned_as_ref {
    ($struct:path => $field:ident) => {
        impl Positioned for $struct {
            fn position(&self) -> Point {
                Positioned::position(self.$field.as_ref())
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nn(value: f64) -> NotNan<f64> {
        NotNan::new(value).unwrap()
    }

    fn point(x: f64, y: f64) -> Point {
        Point::new(nn(x), nn(y))
    }

    fn bounds() -> WorldBounds {
        WorldBounds {
            min_x: nn(0.0),
            max_x: nn(30.0),
            min_y: nn(0.0),
            max_y: nn(30.0),
        }
    }

    #[test]
    fn wraps_points_into_half_open_bounds() {
        assert_eq!(bounds().wrap(point(30.0, -1.0)), point(0.0, 29.0));
    }

    #[test]
    fn distance_uses_shortest_horizontal_and_vertical_wraparound() {
        assert_eq!(
            bounds().distance_between(point(29.0, 15.0), point(1.0, 15.0)),
            Distance(2.0)
        );
        assert_eq!(
            bounds().distance_between(point(15.0, 29.0), point(15.0, 1.0)),
            Distance(2.0)
        );
    }

    #[test]
    fn step_toward_wraps_across_edges() {
        assert_eq!(
            bounds().step_toward(point(29.0, 15.0), point(1.0, 15.0), Distance(1.0)),
            point(0.0, 15.0)
        );
        assert_eq!(
            bounds().step_toward(point(15.0, 29.0), point(15.0, 1.0), Distance(1.0)),
            point(15.0, 0.0)
        );
    }
}
