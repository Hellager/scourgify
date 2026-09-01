//! Models cleanup rules, matching precedence, and import/export.
//!
//! Matching is case-insensitive substring matching. Enabled whitelist rules
//! take precedence over blacklist rules, and the lowest rule ID wins within a
//! rule type. Repository persistence remains in [`crate::db`]; this module owns
//! rule semantics and transfer validation.

mod matcher;
pub(crate) mod transfer;

use serde::{Deserialize, Serialize};

pub(crate) use matcher::{classify, MatchResult};

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
    pub(crate) id: i64,
    pub(crate) keyword: String,
    pub(crate) rule_type: RuleType,
    pub(crate) scope: RuleScope,
    pub(crate) enabled: bool,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct NewRule {
    pub(crate) keyword: String,
    pub(crate) rule_type: RuleType,
    pub(crate) scope: RuleScope,
    pub(crate) enabled: bool,
}
