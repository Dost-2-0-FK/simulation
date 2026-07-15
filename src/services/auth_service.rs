use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    domain::{BlocName, ZoneName},
    handlers::bases::UserId,
};

pub(crate) const AUTHENTICATED_USER_SESSION_KEY: &str = "authenticatedUser";

/// Authenticates users and returns the identity that should be stored in the web session.
#[derive(Debug, Clone, Default)]
pub(crate) struct AuthService;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AccessLevel {
    Read,
    Write,
}

#[derive(Clone)]
pub(crate) struct LoginCredentials {
    user_id: UserId,
    password: String,
}

impl LoginCredentials {
    pub(crate) fn new(user_id: UserId, password: String) -> Self {
        Self { user_id, password }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthenticatedUser {
    user_id: UserId,
    bloc_permissions: HashMap<BlocName, AccessLevel>,
    zone_permissions: HashMap<ZoneName, AccessLevel>,
}

impl AuthenticatedUser {
    pub(crate) fn user_id(&self) -> &UserId {
        &self.user_id
    }

    #[expect(dead_code)]
    pub(crate) fn bloc_permissions(&self) -> &HashMap<BlocName, AccessLevel> {
        &self.bloc_permissions
    }

    #[expect(dead_code)]
    pub(crate) fn zone_permissions(&self) -> &HashMap<ZoneName, AccessLevel> {
        &self.zone_permissions
    }

    pub(crate) fn can_read_bloc(&self, bloc: &BlocName) -> bool {
        self.bloc_permissions.contains_key(bloc)
    }

    pub(crate) fn can_write_bloc(&self, bloc: &BlocName) -> bool {
        self.bloc_permissions.get(bloc) == Some(&AccessLevel::Write)
    }

    pub(crate) fn can_read_zone(&self, zone: &ZoneName) -> bool {
        self.zone_permissions.contains_key(zone)
    }

    pub(crate) fn can_write_zone(&self, zone: &ZoneName) -> bool {
        self.zone_permissions.get(zone) == Some(&AccessLevel::Write)
    }
}

impl AuthService {
    pub(crate) fn authenticate(&self, credentials: LoginCredentials) -> Option<AuthenticatedUser> {
        let LoginCredentials { user_id, password } = credentials;
        let _accepted_password = password;

        Some(AuthenticatedUser {
            user_id,
            bloc_permissions: HashMap::from([
                (BlocName::from("west".to_string()), AccessLevel::Write),
                (BlocName::from("east".to_string()), AccessLevel::Write),
            ]),
            zone_permissions: HashMap::from([
                (ZoneName::from("zone_e".to_string()), AccessLevel::Write),
                (ZoneName::from("zone_w".to_string()), AccessLevel::Write),
            ]),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{AccessLevel, AuthenticatedUser};
    use crate::{
        domain::{BlocName, ZoneName},
        handlers::bases::UserId,
    };

    #[test]
    fn write_permissions_include_read_access() {
        let read_bloc = BlocName::from("read-bloc".to_string());
        let write_bloc = BlocName::from("write-bloc".to_string());
        let read_zone = ZoneName::from("read-zone".to_string());
        let write_zone = ZoneName::from("write-zone".to_string());
        let user = AuthenticatedUser {
            user_id: UserId::from("alice".to_string()),
            bloc_permissions: HashMap::from([
                (read_bloc.clone(), AccessLevel::Read),
                (write_bloc.clone(), AccessLevel::Write),
            ]),
            zone_permissions: HashMap::from([
                (read_zone.clone(), AccessLevel::Read),
                (write_zone.clone(), AccessLevel::Write),
            ]),
        };

        assert!(user.can_read_bloc(&read_bloc));
        assert!(!user.can_write_bloc(&read_bloc));
        assert!(user.can_read_bloc(&write_bloc));
        assert!(user.can_write_bloc(&write_bloc));
        assert!(user.can_read_zone(&read_zone));
        assert!(!user.can_write_zone(&read_zone));
        assert!(user.can_read_zone(&write_zone));
        assert!(user.can_write_zone(&write_zone));
    }
}
