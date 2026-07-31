use actix_session::Session;

use crate::{
    domain::{BlocName, ZoneName},
    error::{Result, UserError},
    services::auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthenticatedUser},
};

pub(crate) mod auth;
pub(crate) mod bases;
pub(crate) mod blocs;
pub(crate) mod combats;
pub(crate) mod costs;
pub(crate) mod placements;
pub(crate) mod production_units;
pub(crate) mod resources;
pub(crate) mod trusts;
pub(crate) mod units;
pub(crate) mod zones;

pub(crate) fn authenticated_user(session: &Session) -> Result<Option<AuthenticatedUser>> {
    session.get(AUTHENTICATED_USER_SESSION_KEY).map_err(|error| {
        log::error!("Error reading authenticated user permissions from session: {error}");
        UserError::InternalError
    })
}

pub(crate) fn can_read_bloc(session: &Session, bloc: &BlocName) -> Result<bool> {
    Ok(authenticated_user(session)?.is_some_and(|user| user.can_read_bloc(bloc)))
}

pub(crate) fn require_bloc_write(session: &Session, bloc: &BlocName) -> Result<()> {
    let user = authenticated_user(session)?.ok_or(UserError::Unauthorized)?;
    user.can_write_bloc(bloc).then_some(()).ok_or(UserError::Forbidden)
}

pub(crate) fn can_read_zone(session: &Session, zone: &ZoneName) -> Result<bool> {
    Ok(authenticated_user(session)?.is_some_and(|user| user.can_read_zone(zone)))
}

pub(crate) fn require_zone_write(session: &Session, zone: &ZoneName) -> Result<()> {
    let user = authenticated_user(session)?.ok_or(UserError::Unauthorized)?;
    user.can_write_zone(zone).then_some(()).ok_or(UserError::Forbidden)
}
