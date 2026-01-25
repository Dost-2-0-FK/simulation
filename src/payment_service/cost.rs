use std::marker::PhantomData;

use serde::Deserialize;

use crate::payment_service::{Money, ResourceValue, Resources};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cost<T> {
    #[serde(skip)]
    _id: PhantomData<T>,
    money: Money,
    resources: Resources,
}

impl<T> Cost<T> {
    pub(crate) fn money(&self) -> Money {
        self.money
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = ResourceValue<'_>> {
        self.resources.into_iter()
    }
}
