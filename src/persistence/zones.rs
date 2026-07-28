use serde::{Deserialize, Serialize};

use crate::domain::{SocialRuleKey, SocialRuleLevel, Zone, ZoneKey};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PersistedSocialRuleLevel {
    key: SocialRuleKey,
    level: SocialRuleLevel,
}

impl PersistedSocialRuleLevel {
    pub(crate) fn key(&self) -> &SocialRuleKey {
        &self.key
    }

    pub(crate) fn level(&self) -> SocialRuleLevel {
        self.level
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PersistedZone {
    #[serde(rename = "_id")]
    id: ZoneKey,
    social_rules: Vec<PersistedSocialRuleLevel>,
}

impl PersistedZone {
    pub(crate) async fn from_zone(zone: &Zone) -> Self {
        Self {
            id: zone.key().clone(),
            social_rules: zone
                .social_rules()
                .await
                .into_iter()
                .map(|assignment| PersistedSocialRuleLevel {
                    key: assignment.rule().key().clone(),
                    level: assignment.level(),
                })
                .collect(),
        }
    }

    pub(crate) fn id(&self) -> &ZoneKey {
        &self.id
    }

    pub(crate) fn social_rules(&self) -> &[PersistedSocialRuleLevel] {
        &self.social_rules
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use super::PersistedZone;
    use crate::{
        domain::{
            Bloc, BlocKey, BlocName, Chance, SocialRule, SocialRuleKey, SocialRuleLevel, SocialRuleName, Zone, ZoneKey,
            ZoneName, ZoneSocialRule,
        },
        services::credit_exchange_service::Share,
    };

    #[tokio::test]
    async fn persists_service_facing_rule_keys_and_current_levels() {
        let bloc_key = BlocKey::from("bloc".to_string());
        let bloc_name = BlocName::from("Bloc".to_string());
        let level = serde_json::from_value::<SocialRuleLevel>(serde_json::json!(1)).unwrap();
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
            vec![ZoneSocialRule::new(
                SocialRule::new(
                    SocialRuleKey::from("rule-key".to_string()),
                    SocialRuleName::from("Rule Name".to_string()),
                    serde_json::from_value(serde_json::json!(-2)).unwrap(),
                    serde_json::from_value(serde_json::json!(2)).unwrap(),
                    None,
                    None,
                ),
                level,
            )],
        );

        let document = mongodb::bson::to_document(&PersistedZone::from_zone(&zone).await).unwrap();

        assert_eq!(document.get_str("_id").unwrap(), "zone-key");
        let social_rule = document.get_array("social_rules").unwrap()[0].as_document().unwrap();
        assert_eq!(social_rule.get_str("key").unwrap(), "rule-key");
        assert_eq!(social_rule.get_i32("level").unwrap(), 1);
        assert!(!social_rule.contains_key("name"));
    }
}
