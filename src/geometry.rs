#[derive(Debug, Clone, Copy)]
pub(crate) struct Point {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub(crate) struct Distance(f64);

pub(crate) trait Positioned {
    fn position(&self) -> Point;

    fn distance_to(&self, other: &impl Positioned) -> Distance {
        let other_position = other.position();
        let self_position = self.position();

        let distance_x = self_position.x - other_position.x;
        let distance_y = self_position.y - other_position.y;
        let distance = (distance_x.powi(2) + distance_y.powi(2)).sqrt();

        Distance(distance)
    }
}
