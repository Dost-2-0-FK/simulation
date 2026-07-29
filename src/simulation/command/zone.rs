use std::sync::Arc;

use tokio::sync::oneshot::Sender;

use crate::{
    domain::{SocialRuleLevel, SocialRuleName, SocialRulePatchError, Zone, ZoneKey},
    error::UserError,
    services::auth_service::{AuthService, AuthServiceResponseError},
};

pub(crate) fn get(resp: Sender<Vec<Arc<Zone>>>, zones: impl Iterator<Item = Arc<Zone>>) {
    let _ = resp.send(zones.collect());
}

pub(crate) async fn patch(
    response: Sender<core::result::Result<(), UserError>>,
    mut zones: impl Iterator<Item = Arc<Zone>>,
    auth_service: &AuthService,
    id: &ZoneKey,
    social_rules: &[(SocialRuleName, SocialRuleLevel)],
) {
    let result = async {
        let zone = zones.find(|zone| zone.key() == id).ok_or(UserError::NotFound("Zone"))?;
        let changes = zone
            .social_rule_level_changes(social_rules)
            .await
            .map_err(patch_error)?;

        for change in changes {
            auth_service
                .publish_social_rule_update(change.key(), change.level(), change.old_level(), id)
                .await
                .map_err(|error| {
                    if let Some(response) = error.downcast_ref::<AuthServiceResponseError>() {
                        UserError::AuthService {
                            status: response.status().as_u16(),
                            body: response.body().to_string(),
                        }
                    } else {
                        log::error!("auth-service error while publishing social-rule update: {error:#}");
                        UserError::InternalError
                    }
                })?;
            zone.patch_social_rule_levels(&[(change.name().clone(), change.level())])
                .await
                .expect("validated social-rule change remains valid while the state loop processes it");
        }

        Ok(())
    };
    let result = result.await;
    let _ = response.send(result);
}

fn patch_error(error: SocialRulePatchError) -> UserError {
    match error {
        SocialRulePatchError::DuplicateRule(_) => UserError::BadRequest("social rule occurs more than once"),
        SocialRulePatchError::UnassignedRule(_) => UserError::BadRequest("social rule is not assigned to this zone"),
        SocialRulePatchError::LevelOutOfRange { .. } => {
            UserError::BadRequest("social rule level is outside its configured range")
        }
    }
}
