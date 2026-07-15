use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RuleType {
    Whitelist,
    Blacklist,
}

impl RuleType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Whitelist => "whitelist",
            Self::Blacklist => "blacklist",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct Rule {
    pub id: i64,
    pub keyword: String,
    pub rule_type: RuleType,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct NewRule {
    pub keyword: String,
    pub rule_type: RuleType,
    pub enabled: bool,
}
