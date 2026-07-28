use actix_session::Session;
use actix_web::{HttpResponse, Responder, get, patch, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{BlocName, NameMappings, SocialRuleFactorPerLevel, SocialRuleLevel, SocialRuleName, Zone, ZoneName},
    error::{Result, UserError},
    handlers::require_zone_write,
    simulation::Command,
};

const ZONES: &str = "zones";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZoneResponse {
    name: ZoneName,
    bloc: BlocName,
    social_rules: Vec<SocialRuleResponse>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SocialRuleResponse {
    name: SocialRuleName,
    level: SocialRuleLevel,
    min_level: SocialRuleLevel,
    max_level: SocialRuleLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    trust_production_factor_per_level: Option<SocialRuleFactorPerLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    production_unit_factor_per_level: Option<SocialRuleFactorPerLevel>,
}

impl ZoneResponse {
    async fn new(zone: &Zone) -> Self {
        Self {
            name: zone.name().clone(),
            bloc: zone.bloc_name().clone(),
            social_rules: zone
                .social_rules()
                .await
                .into_iter()
                .map(|assignment| SocialRuleResponse {
                    name: assignment.rule().name().clone(),
                    level: assignment.level(),
                    min_level: assignment.rule().min_level(),
                    max_level: assignment.rule().max_level(),
                    trust_production_factor_per_level: assignment.rule().trust_production_factor_per_level(),
                    production_unit_factor_per_level: assignment.rule().production_unit_factor_per_level(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PatchZoneBody {
    social_rules: Vec<PatchSocialRuleLevel>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PatchSocialRuleLevel {
    name: SocialRuleName,
    level: SocialRuleLevel,
}

/// List all zones.
#[utoipa::path(
    operation_id = "listZones",
    tag = ZONES,
    responses(
        (status = 200, description = "All existing zones", body = [ZoneResponse]),
        (status = 500, description = "Failed to retrieve zones", body = String, content_type = "text/html")
    )
)]
#[get("/zones")]
pub(crate) async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetZones(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let zones = receiver.await.map_err(|e| {
        log::error!("Error receiving zones: {e}");
        UserError::InternalError
    })?;

    let mut responses = Vec::with_capacity(zones.len());
    for zone in zones {
        responses.push(ZoneResponse::new(&zone).await);
    }

    Ok(HttpResponse::Ok().json(responses))
}

/// Update the levels of social rules assigned to a zone.
#[utoipa::path(
    operation_id = "patchZone",
    tag = ZONES,
    responses(
        (status = 200, description = "Zone social-rule levels updated successfully"),
        (status = 400, description = "Duplicate, unassigned, or out-of-range social rule", body = String, content_type = "text/html"),
        (status = 401, description = "Missing or invalid user credentials", body = String, content_type = "text/html"),
        (status = 403, description = "Missing required zone-write permission", body = String, content_type = "text/html"),
        (status = 404, description = "Zone not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to update the zone", body = String, content_type = "text/html")
    )
)]
#[patch("/zones/{id}")]
pub(crate) async fn patch(
    session: Session,
    path: web::Path<ZoneName>,
    body: web::Json<PatchZoneBody>,
    tx: web::Data<mpsc::Sender<Command>>,
    mappings: web::Data<NameMappings>,
) -> Result<impl Responder> {
    let name = path.into_inner();
    let id = mappings.zone_key(&name).cloned().ok_or(UserError::NotFound("Zone"))?;
    require_zone_write(&session, &name)?;

    let social_rules = body
        .social_rules
        .iter()
        .map(|patch| (patch.name.clone(), patch.level))
        .collect();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::PatchZone {
        id,
        social_rules,
        response: sender,
    })
    .await
    .map_err(|error| {
        log::error!("Error sending zone patch command: {error}");
        UserError::InternalError
    })?;

    let result = receiver.await.map_err(|error| {
        log::error!("Error receiving zone patch result: {error}");
        UserError::InternalError
    })?;
    result.map(|()| HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use super::{PatchZoneBody, ZoneResponse};
    use crate::{
        domain::{
            Bloc, BlocKey, BlocName, Chance, SocialRule, SocialRuleFactorPerLevel, SocialRuleKey, SocialRuleLevel,
            SocialRuleName, Zone, ZoneKey, ZoneName, ZoneSocialRule,
        },
        services::credit_exchange_service::Share,
    };

    #[tokio::test]
    async fn zone_response_exposes_social_rule_names_and_configuration_but_not_keys() {
        let bloc_key = BlocKey::from("bloc-key".to_string());
        let bloc_name = BlocName::from("Bloc".to_string());
        let level = serde_json::from_value::<SocialRuleLevel>(serde_json::json!(1)).unwrap();
        let rule = SocialRule::new(
            SocialRuleKey::from("service-key".to_string()),
            SocialRuleName::from("Frontend Name".to_string()),
            serde_json::from_value(serde_json::json!(-2)).unwrap(),
            serde_json::from_value(serde_json::json!(2)).unwrap(),
            Some(serde_json::from_value::<SocialRuleFactorPerLevel>(serde_json::json!(-0.025)).unwrap()),
            None,
        );
        let zone = Zone::new_with_social_rules(
            ZoneKey::from("zone-key".to_string()),
            ZoneName::from("Zone".to_string()),
            bloc_key.clone(),
            bloc_name.clone(),
            Arc::new(RwLock::new(Bloc::new(
                bloc_key,
                bloc_name,
                Chance::new(1),
                Share::default(),
            ))),
            vec![ZoneSocialRule::new(rule, level)],
        );

        let response = serde_json::to_value(ZoneResponse::new(&zone).await).unwrap();

        assert_eq!(response["socialRules"][0]["name"], "Frontend Name");
        assert_eq!(response["socialRules"][0]["level"], 1);
        assert_eq!(response["socialRules"][0]["minLevel"], -2);
        assert_eq!(response["socialRules"][0]["maxLevel"], 2);
        let trust_factor = response["socialRules"][0]["trustProductionFactorPerLevel"]
            .as_f64()
            .unwrap();
        assert!((trust_factor - -0.025).abs() < 0.000_001);
        assert!(response["socialRules"][0].get("key").is_none());
    }

    #[test]
    fn zone_patch_accepts_names_and_rejects_service_keys() {
        let body = serde_json::from_value::<PatchZoneBody>(serde_json::json!({
            "socialRules": [{ "name": "Frontend Name", "level": 1 }]
        }))
        .expect("frontend names are accepted");
        assert_eq!(body.social_rules[0].name.to_string(), "Frontend Name");

        assert!(
            serde_json::from_value::<PatchZoneBody>(serde_json::json!({
                "socialRules": [{ "key": "service-key", "level": 1 }]
            }))
            .is_err()
        );
    }
}
