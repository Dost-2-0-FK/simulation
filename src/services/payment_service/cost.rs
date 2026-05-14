use std::{any::type_name, marker::PhantomData};

use derive_more::Display;
use serde::Deserialize;

use crate::services::payment_service::{Money, ResourceValue, Resources};

#[derive(Debug, Deserialize, Display)]
#[display("{}: {} ({})", type_name::<T>(), money, resources)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cost<T> {
    #[serde(skip)]
    _id: PhantomData<T>,
    money: Money,
    resources: Resources,
}

impl<T> Cost<T> {
    #[expect(dead_code)]
    pub(crate) fn money(&self) -> Money {
        self.money
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = ResourceValue<'_>> {
        self.resources.into_iter()
    }
}
