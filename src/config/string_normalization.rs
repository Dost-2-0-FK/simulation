use serde::{Deserialize, Deserializer};

#[derive(Debug, derive_more::Deref, Hash, PartialEq, Eq, derive_more::Display)]
pub(super) struct LowercaseString(String);

impl<'de> Deserialize<'de> for LowercaseString {
    fn deserialize<D>(d: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        Ok(LowercaseString(s.to_lowercase()))
    }
}

#[derive(Debug, derive_more::Deref, Deserialize)]
pub(super) struct VecLowercaseString(Vec<LowercaseString>);

impl std::fmt::Display for VecLowercaseString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();
        for entry in self.iter() {
            list.entry(&entry.0);
        }
        list.finish()
    }
}
