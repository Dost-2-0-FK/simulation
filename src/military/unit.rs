use crate::{
    military::base::MilitaryBase,
    money::{Money, ResourceValue},
};

/// Associated with a [MilitaryBase] and a [Bloc]. The [Bloc] association is implicit.
#[derive(Debug, Clone)]
pub(crate) struct MilitaryUnit {
    base: MilitaryBase,
}

#[derive(Debug, Clone)]
pub(crate) struct MilitaryUnitCost {
    money: Money,
    resource: HashSet<ResourceValue>,
}
