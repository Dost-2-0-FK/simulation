use std::sync::Arc;

use tokio::sync::oneshot::Sender;

use crate::{
    domain::{SocialRuleLevel, SocialRuleName, SocialRulePatchError, Zone, ZoneKey},
    error::UserError,
};

pub(crate) fn get(resp: Sender<Vec<Arc<Zone>>>, zones: impl Iterator<Item = Arc<Zone>>) {
    let _ = resp.send(zones.collect());
}

pub(crate) async fn patch(
    response: Sender<core::result::Result<(), UserError>>,
    mut zones: impl Iterator<Item = Arc<Zone>>,
    id: &ZoneKey,
    social_rules: &[(SocialRuleName, SocialRuleLevel)],
) {
    let result = match zones.find(|zone| zone.key() == id) {
        None => Err(UserError::NotFound("Zone")),
        Some(zone) => zone
            .patch_social_rule_levels(social_rules)
            .await
            .map_err(|error| match error {
                SocialRulePatchError::DuplicateRule(_) => UserError::BadRequest("social rule occurs more than once"),
                SocialRulePatchError::UnassignedRule(_) => {
                    UserError::BadRequest("social rule is not assigned to this zone")
                }
                SocialRulePatchError::LevelOutOfRange { .. } => {
                    UserError::BadRequest("social rule level is outside its configured range")
                }
            }),
    };
    let _ = response.send(result);
}
