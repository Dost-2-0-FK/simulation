#[derive(Debug, Clone)]
pub(crate) struct Money(f32);

#[derive(Debug, Clone)]
pub(crate) struct ResouceName(String);

#[derive(Debug, Clone)]
pub(crate) struct ResourceValue(ResouceName, f32);
