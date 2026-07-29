use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result, anyhow};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use url::Url;

use derive_more::{Display, Error};

use crate::{
    domain::{
        BlocKey, BlocName, CharacterKey, CharacterName, NameMappings, PlacementId, SocialRuleKey, SocialRuleLevel,
        ZoneKey, ZoneName,
    },
    handlers::bases::Financing,
    services::credit_exchange_service::ResourceName,
};

pub(crate) const AUTHENTICATED_USER_SESSION_KEY: &str = "authenticatedUser";

/// Authenticates users and returns the identity that should be stored in the web session.
#[derive(Debug, Clone)]
pub(crate) struct AuthService {
    client: reqwest::Client,
    url: Url,
    name_mappings: Arc<NameMappings>,
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
    placement_id: &'a PlacementId,
    object_type: FinancedObject,
    trust_type: &'a str,
}

#[derive(Debug, Serialize)]
struct SocialRuleUpdateRequest<'a> {
    name: &'a SocialRuleKey,
    level: SocialRuleLevel,
    #[serde(rename = "levelOld")]
    old_level: SocialRuleLevel,
    zone: &'a ZoneKey,
}

#[derive(Debug, Display, Error)]
#[display("auth service returned {status}: {body}")]
pub(crate) struct AuthServiceResponseError {
    status: StatusCode,
    body: String,
}

impl AuthServiceResponseError {
    fn new(status: StatusCode, body: String) -> Self {
        Self { status, body }
    }

    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AccessLevel {
    Read,
    Write,
}

#[derive(Clone, Serialize)]
pub(crate) struct LoginCredentials {
    password: CharacterKey,
}

impl LoginCredentials {
    pub(crate) fn new(password: CharacterKey) -> Self {
        Self { password }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthenticatedUser {
    #[serde(rename = "userId")]
    character_name: CharacterName,
    bloc_permissions: HashMap<BlocName, AccessLevel>,
    zone_permissions: HashMap<ZoneName, AccessLevel>,
}

#[derive(Debug, Deserialize)]
struct AuthenticationResponse {
    #[serde(rename = "userID")]
    character_key: CharacterKey,
    #[serde(rename = "blockPermissions")]
    bloc_permissions: HashMap<BlocKey, AccessLevel>,
    #[serde(rename = "zonePermissions")]
    zone_permissions: HashMap<ZoneKey, AccessLevel>,
}

impl AuthenticatedUser {
    pub(crate) fn character_name(&self) -> &CharacterName {
        &self.character_name
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
    pub(crate) fn new(url: Url, name_mappings: Arc<NameMappings>) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
            name_mappings,
        }
    }

    pub(crate) async fn authenticate(&self, credentials: LoginCredentials) -> Result<Option<AuthenticatedUser>> {
        let endpoint = self
            .url
            .join("api/users/auth")
            .context("building auth-service authentication endpoint")?;
        let response = self
            .client
            .post(endpoint)
            .json(&credentials)
            .send()
            .await
            .context("requesting authentication")?;

        if matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ) {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|err| err.to_string());
            return Err(anyhow!("authentication failed with {status}: {body}"));
        }

        let response = response
            .json::<AuthenticationResponse>()
            .await
            .context("deserializing auth-service authentication response")?;

        self.map_authenticated_user(response).map(Some)
    }

    fn map_authenticated_user(&self, response: AuthenticationResponse) -> Result<AuthenticatedUser> {
        let character_name = self
            .name_mappings
            .character_name(&response.character_key)
            .cloned()
            .with_context(|| format!("auth service returned unknown character key {}", response.character_key))?;
        let bloc_permissions = response
            .bloc_permissions
            .into_iter()
            .map(|(key, access)| {
                self.name_mappings
                    .bloc_name(&key)
                    .cloned()
                    .map(|name| (name, access))
                    .with_context(|| format!("auth service returned unknown bloc key {key}"))
            })
            .collect::<Result<HashMap<_, _>>>()?;
        let zone_permissions = response
            .zone_permissions
            .into_iter()
            .map(|(key, access)| {
                self.name_mappings
                    .zone_name(&key)
                    .cloned()
                    .map(|name| (name, access))
                    .with_context(|| format!("auth service returned unknown zone key {key}"))
            })
            .collect::<Result<HashMap<_, _>>>()?;

        Ok(AuthenticatedUser {
            character_name,
            bloc_permissions,
            zone_permissions,
        })
    }

    /// Verifies that every listed financier consents before any payment is booked.
    pub(crate) async fn verify_financing(
        &self,
        placement_id: &PlacementId,
        object_type: FinancedObject,
        financing: &[Financing],
        trust_type: Option<&ResourceName>,
    ) -> Result<bool> {
        for financing in financing {
            let endpoint = self
                .url
                .join(&format!("api/users/{}/verify", financing.financier.as_str()))
                .context("building auth-service financing verification endpoint")?;
            let request = FinancingVerificationRequest {
                placement_id,
                object_type,
                trust_type: trust_type.map(ResourceName::as_str).unwrap_or_default(),
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

    pub(crate) async fn publish_social_rule_update(
        &self,
        rule: &SocialRuleKey,
        level: SocialRuleLevel,
        old_level: SocialRuleLevel,
        zone: &ZoneKey,
    ) -> Result<()> {
        let endpoint = self.social_rule_update_endpoint(rule)?;
        let response = self
            .client
            .post(endpoint)
            .json(&SocialRuleUpdateRequest {
                name: rule,
                level,
                old_level,
                zone,
            })
            .send()
            .await
            .context("publishing social-rule update")?;

        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_else(|err| err.to_string());
        Err(AuthServiceResponseError::new(status, body).into())
    }

    fn social_rule_update_endpoint(&self, rule: &SocialRuleKey) -> Result<Url> {
        let mut endpoint = self
            .url
            .join("api/events/social/")
            .context("building auth-service social-rule update endpoint")?;
        endpoint
            .path_segments_mut()
            .map_err(|()| anyhow!("auth-service URL cannot contain path segments"))?
            .pop_if_empty()
            .push(rule.as_str());
        Ok(endpoint)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use super::{
        AccessLevel, AuthenticatedUser, AuthenticationResponse, FinancedObject, FinancingVerificationRequest,
        LoginCredentials, SocialRuleUpdateRequest,
    };
    use crate::{
        domain::{
            BlocKey, BlocName, CharacterKey, CharacterName, NameMappings, PlacementId, SocialRuleKey,
            SocialRuleLevel, ZoneKey, ZoneName,
        },
    };

    #[test]
    fn write_permissions_include_read_access() {
        let read_bloc = BlocName::from("read-bloc".to_string());
        let write_bloc = BlocName::from("write-bloc".to_string());
        let read_zone = ZoneName::from("read-zone".to_string());
        let write_zone = ZoneName::from("write-zone".to_string());
        let user = AuthenticatedUser {
            character_name: CharacterName::from("Alice".to_string()),
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
    fn authentication_request_matches_the_auth_service_contract() {
        assert_eq!(
            serde_json::to_value(LoginCredentials::new(CharacterKey::from(
                "encrypted-public-key".to_string(),
            )))
            .unwrap(),
            serde_json::json!({ "password": "encrypted-public-key" })
        );
    }

    #[test]
    fn authentication_response_matches_the_auth_service_contract() {
        let response = serde_json::from_value::<AuthenticationResponse>(serde_json::json!({
            "userID": "alice",
            "blockPermissions": {
                "north": "write",
                "south": "read"
            },
            "zonePermissions": {
                "alpha": "write",
                "beta": "read"
            }
        }))
        .unwrap();
        let mappings = NameMappings::new(
            HashMap::from([
                (BlocKey::from("north".to_string()), BlocName::from("North".to_string())),
                (BlocKey::from("south".to_string()), BlocName::from("South".to_string())),
            ]),
            HashMap::from([
                (ZoneKey::from("alpha".to_string()), ZoneName::from("Alpha".to_string())),
                (ZoneKey::from("beta".to_string()), ZoneName::from("Beta".to_string())),
            ]),
            HashMap::from([(
                CharacterKey::from("alice".to_string()),
                CharacterName::from("Alice".to_string()),
            )]),
        );
        let service = super::AuthService::new("http://localhost".parse().unwrap(), Arc::new(mappings));
        let user = service.map_authenticated_user(response).unwrap();

        assert_eq!(user.character_name().to_string(), "Alice");
        assert!(user.can_write_bloc(&BlocName::from("North".to_string())));
        assert!(user.can_read_bloc(&BlocName::from("South".to_string())));
        assert!(user.can_write_zone(&ZoneName::from("Alpha".to_string())));
        assert!(user.can_read_zone(&ZoneName::from("Beta".to_string())));

        let frontend = serde_json::to_value(user).unwrap();
        assert_eq!(frontend["userId"], "Alice");
        assert_eq!(frontend["blocPermissions"]["North"], "write");
        assert_eq!(frontend["zonePermissions"]["Alpha"], "write");
    }

    #[test]
    fn financing_verification_request_matches_the_auth_service_contract() {
        let placement_id = serde_json::from_value::<PlacementId>(serde_json::json!("placement-1")).unwrap();
        let trust_request = FinancingVerificationRequest {
            placement_id: &placement_id,
            object_type: FinancedObject::Trust,
            trust_type: "iron",
        };
        let base_request = FinancingVerificationRequest {
            placement_id: &placement_id,
            object_type: FinancedObject::Base,
            trust_type: "",
        };

        assert_eq!(
            serde_json::to_value(trust_request).unwrap(),
            serde_json::json!({
                "placementId": "placement-1",
                "objectType": "trust",
                "trustType": "iron"
            })
        );
        assert_eq!(
            serde_json::to_value(base_request).unwrap(),
            serde_json::json!({
                "placementId": "placement-1",
                "objectType": "base",
                "trustType": ""
            })
        );
    }

    #[test]
    fn social_rule_update_request_matches_the_auth_service_contract() {
        let rule = SocialRuleKey::from("social-rule".to_string());
        let level = serde_json::from_value::<SocialRuleLevel>(serde_json::json!(2)).unwrap();
        let zone = ZoneKey::from("zone".to_string());

        assert_eq!(
            serde_json::to_value(SocialRuleUpdateRequest {
                name: &rule,
                level,
                old_level: serde_json::from_value::<SocialRuleLevel>(serde_json::json!(0)).unwrap(),
                zone: &zone,
            })
            .unwrap(),
            serde_json::json!({
                "name": "social-rule",
                "level": 2,
                "levelOld": 0,
                "zone": "zone"
            })
        );
    }

    #[test]
    fn social_rule_key_is_one_encoded_endpoint_segment() {
        let mappings = NameMappings::new(HashMap::new(), HashMap::new(), HashMap::new());
        let service = super::AuthService::new("http://localhost/".parse().unwrap(), Arc::new(mappings));

        let endpoint = service
            .social_rule_update_endpoint(&SocialRuleKey::from("rule/with space".to_string()))
            .unwrap();

        assert_eq!(
            endpoint.as_str(),
            "http://localhost/api/events/social/rule%2Fwith%20space"
        );
    }
}
