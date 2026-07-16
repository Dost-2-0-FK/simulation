use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    domain::{BlocName, Loot, PlacementId, ZoneName},
    handlers::bases::{Financing, UserId},
    services::credit_exchange_service::Cost,
};

pub(crate) const AUTHENTICATED_USER_SESSION_KEY: &str = "authenticatedUser";

/// Authenticates users and returns the identity that should be stored in the web session.
#[derive(Debug, Clone)]
pub(crate) struct AuthService {
    client: reqwest::Client,
    url: Url,
    bloc_permissions: HashMap<BlocName, AccessLevel>,
    zone_permissions: HashMap<ZoneName, AccessLevel>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FinancedObject {
    Trust,
    Base,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FinancingVerificationRequest<'a> {
    user_id: &'a UserId,
    financier_id: &'a UserId,
    placement_id: &'a PlacementId,
    object_type: FinancedObject,
    share: crate::services::credit_exchange_service::Share,
    cost: Loot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AccessLevel {
    Read,
    Write,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
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

    #[cfg(test)]
    pub(crate) fn bloc_permissions(&self) -> &HashMap<BlocName, AccessLevel> {
        &self.bloc_permissions
    }

    #[cfg(test)]
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
    /// Temporarily pass in the configured blocs and zones to hand out write permissions for all of them.
    pub(crate) fn new(
        url: Url,
        blocs: impl IntoIterator<Item = BlocName>,
        zones: impl IntoIterator<Item = ZoneName>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
            bloc_permissions: blocs.into_iter().map(|bloc| (bloc, AccessLevel::Write)).collect(),
            zone_permissions: zones.into_iter().map(|zone| (zone, AccessLevel::Write)).collect(),
        }
    }

    pub(crate) fn authenticate(&self, credentials: LoginCredentials) -> Option<AuthenticatedUser> {
        let LoginCredentials { user_id, password } = credentials;
        let _accepted_password = password;

        Some(AuthenticatedUser {
            user_id,
            bloc_permissions: self.bloc_permissions.clone(),
            zone_permissions: self.zone_permissions.clone(),
        })
    }

    /// Verifies that every listed financier consents before any payment is booked.
    pub(crate) async fn verify_financing<T>(
        &self,
        user_id: &UserId,
        placement_id: &PlacementId,
        object_type: FinancedObject,
        financing: &[Financing],
        cost: &Cost<T>,
    ) -> Result<bool> {
        for financing in financing {
            let endpoint = self
                .url
                .join(&format!("api/users/{}/financing/verify", financing.financier.as_str()))
                .context("building auth-service financing verification endpoint")?;
            let request = FinancingVerificationRequest {
                user_id,
                financier_id: &financing.financier,
                placement_id,
                object_type,
                share: financing.share,
                cost: Loot::from_cost_share(cost, financing.share),
            };

            let response = self
                .client
                .post(endpoint)
                .json(&request)
                .send()
                .await
                .with_context(|| format!("requesting financing approval from {}", financing.financier.as_str()))?;

            if response.status().is_success() {
                continue;
            }
            if response.status() == StatusCode::FORBIDDEN {
                return Ok(false);
            }

            let status = response.status();
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            return Err(anyhow!(
                "financing verification for {} failed with {status}: {body}",
                financing.financier.as_str()
            ));
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        AccessLevel, AuthService, AuthenticatedUser, FinancedObject, FinancingVerificationRequest, LoginCredentials,
    };
    use crate::{
        domain::{BlocName, Loot, PlacementId, ZoneName},
        handlers::bases::UserId,
        services::credit_exchange_service::Share,
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
        assert!(!user.can_write_bloc(&BlocName::from("unlisted-bloc".to_string())));
        assert!(user.can_read_zone(&read_zone));
        assert!(!user.can_write_zone(&read_zone));
        assert!(user.can_read_zone(&write_zone));
        assert!(user.can_write_zone(&write_zone));
        assert!(!user.can_write_zone(&ZoneName::from("unlisted-zone".to_string())));
    }

    #[test]
    fn authentication_grants_write_access_to_every_configured_bloc_and_zone() {
        let blocs = [BlocName::from("north".to_string()), BlocName::from("south".to_string())];
        let zones = [ZoneName::from("alpha".to_string()), ZoneName::from("beta".to_string())];
        let service = AuthService::new("http://127.0.0.1:18081".parse().unwrap(), blocs.clone(), zones.clone());

        let user = service
            .authenticate(LoginCredentials::new(
                UserId::from("alice".to_string()),
                "secret".to_string(),
            ))
            .unwrap();

        assert_eq!(user.bloc_permissions().len(), blocs.len());
        assert!(
            user.bloc_permissions()
                .values()
                .all(|level| *level == AccessLevel::Write)
        );
        assert_eq!(user.zone_permissions().len(), zones.len());
        assert!(
            user.zone_permissions()
                .values()
                .all(|level| *level == AccessLevel::Write)
        );
        assert!(blocs.iter().all(|bloc| user.can_write_bloc(bloc)));
        assert!(zones.iter().all(|zone| user.can_write_zone(zone)));
        assert!(!user.can_write_bloc(&BlocName::from("unconfigured".to_string())));
        assert!(!user.can_write_zone(&ZoneName::from("unconfigured".to_string())));
    }

    #[test]
    fn financing_verification_request_matches_the_auth_service_contract() {
        let user_id = UserId::from("builder".to_string());
        let financier_id = UserId::from("sponsor".to_string());
        let placement_id = serde_json::from_value::<PlacementId>(serde_json::json!("placement-1")).unwrap();
        let request = FinancingVerificationRequest {
            user_id: &user_id,
            financier_id: &financier_id,
            placement_id: &placement_id,
            object_type: FinancedObject::Trust,
            share: Share::from(0.25),
            cost: serde_json::from_value::<Loot>(serde_json::json!({
                "money": 3.0,
                "resources": { "iron": 2.0 }
            }))
            .unwrap(),
        };

        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "userId": "builder",
                "financierId": "sponsor",
                "placementId": "placement-1",
                "objectType": "trust",
                "share": 0.25,
                "cost": {
                    "money": 3.0,
                    "resources": { "iron": 2.0 }
                }
            })
        );
    }
}
