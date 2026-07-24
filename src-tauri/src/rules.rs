use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RuleType {
    Whitelist,
    Blacklist,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RuleScope {
    All,
    Files,
    Folders,
}

impl RuleScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Files => "files",
            Self::Folders => "folders",
        }
    }

    pub(crate) fn applies_to(self, item_scope: Self) -> bool {
        self == Self::All || self == item_scope
    }
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
    pub scope: RuleScope,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct NewRule {
    pub keyword: String,
    pub rule_type: RuleType,
    pub scope: RuleScope,
    pub enabled: bool,
}
