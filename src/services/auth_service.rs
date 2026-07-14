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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
