#[derive(Debug, Clone)]
pub(crate) struct ZoneName(String);

#[derive(Debug, Clone)]
pub(crate) struct Zone(ZoneName, Bloc);

/// Every unit belongs to a [Zone], which belongs to a [Bloc], which implies a [Chance].
/// When two units of a different [Bloc] meet, they fight: For each unit, a die is rolled, i.e., a  uniform random draw
/// of [0, [Chance]]. On 0, the other unit is eliminated. If both dice show 0, both units are eliminated.
#[derive(Debug, Clone)]
struct Chance(u32);

#[derive(Debug, Clone)]
pub(crate) struct BlocName(String);

#[derive(Debug, Clone)]
pub(crate) struct Bloc(BlocName, Chance);
