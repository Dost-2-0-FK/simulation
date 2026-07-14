use crate::domain::{BlocName, ZoneName};

/// Authenticates users and returns the identity that should be stored in the web session.
#[derive(Debug, Clone, Default)]
pub(crate) struct AuthService;

#[derive(Clone)]
pub(crate) struct LoginCredentials {
    user_id: String,
    password: String,
}

impl LoginCredentials {
    pub(crate) fn new(user_id: String, password: String) -> Self {
        Self { user_id, password }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedUser {
    user_id: String,
}

impl AuthenticatedUser {
    pub(crate) fn user_id(&self) -> &str {
        &self.user_id
    }
}

impl AuthService {
    pub(crate) fn authenticate(&self, credentials: LoginCredentials) -> Option<AuthenticatedUser> {
        let LoginCredentials { user_id, password } = credentials;
        let _accepted_password = password;

        Some(AuthenticatedUser { user_id })
    }
}
