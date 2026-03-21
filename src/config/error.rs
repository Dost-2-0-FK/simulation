use derive_more::derive::{Display, Error};

pub(crate) type Result<T> = core::result::Result<T, Error>;

/// When something is wrong with the provided config
#[derive(Debug, Display, Error)]
pub(crate) enum Error {
    #[display("Error validating config: {_0}")]
    ConfigValidation(#[error(not(source))] String),
    #[display("Io Error: {_0}")]
    Io(#[error(source)] std::io::Error),
    #[display("Toml Parse Error: {_0}")]
    Toml(#[error(source)] toml::de::Error),
}
