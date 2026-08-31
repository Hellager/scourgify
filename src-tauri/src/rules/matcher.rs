use serde::Serialize;

use crate::rules::{Rule, RuleScope, RuleType};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MatchResult {
    Protected { rule_id: i64, keyword: String },
    Targeted { rule_id: i64, keyword: String },
    Neutral,
}

pub fn classify(path: &str, item_scope: RuleScope, rules: &[Rule]) -> MatchResult {
    let path = path.to_lowercase();
    if let Some(rule) = first_match(&path, item_scope, rules, RuleType::Whitelist) {
        return MatchResult::Protected {
            rule_id: rule.id,
            keyword: rule.keyword.clone(),
        };
    }

    match first_match(&path, item_scope, rules, RuleType::Blacklist) {
        Some(rule) => MatchResult::Targeted {
            rule_id: rule.id,
            keyword: rule.keyword.clone(),
        },
        None => MatchResult::Neutral,
    }
}

fn first_match<'a>(
    path: &str,
    item_scope: RuleScope,
    rules: &'a [Rule],
    rule_type: RuleType,
) -> Option<&'a Rule> {
    rules
        .iter()
        .filter(|rule| {
            rule.enabled
                && rule.rule_type == rule_type
                && rule.scope.applies_to(item_scope)
                && !rule.keyword.is_empty()
                && path.contains(&rule.keyword.to_lowercase())
        })
        .min_by_key(|rule| rule.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_case_insensitive_blacklist_match() {
        let rules = [rule(1, "TEMP", RuleType::Blacklist, true)];

        assert_eq!(
            classify(
                r"C:\Users\test\AppData\Temp\report.txt",
                RuleScope::Files,
                &rules,
            ),
            MatchResult::Targeted {
                rule_id: 1,
                keyword: "TEMP".to_string(),
            }
        );
    }

    #[test]
    fn whitelist_takes_priority_over_blacklist() {
        let rules = [
            rule(1, "Projects", RuleType::Blacklist, true),
            rule(2, "report", RuleType::Whitelist, true),
        ];

        assert_eq!(
            classify(r"C:\Projects\report.txt", RuleScope::Files, &rules),
            MatchResult::Protected {
                rule_id: 2,
                keyword: "report".to_string(),
            }
        );
    }

    #[test]
    fn lowest_id_wins_for_same_rule_type_regardless_of_input_order() {
        let rules = [
            rule(8, "Temp", RuleType::Blacklist, true),
            rule(3, "AppData", RuleType::Blacklist, true),
        ];

        assert_eq!(
            classify(r"C:\AppData\Temp\report.txt", RuleScope::Files, &rules,),
            MatchResult::Targeted {
                rule_id: 3,
                keyword: "AppData".to_string(),
            }
        );
    }

    #[test]
    fn ignores_disabled_and_empty_rules() {
        let rules = [
            rule(1, "Temp", RuleType::Blacklist, false),
            rule(2, "", RuleType::Whitelist, true),
        ];

        assert_eq!(
            classify(r"C:\Users\test\Temp\report.txt", RuleScope::Files, &rules,),
            MatchResult::Neutral
        );
    }

    #[test]
    fn returns_neutral_when_no_rule_matches() {
        let rules = [rule(1, "Temp", RuleType::Blacklist, true)];

        assert_eq!(
            classify(
                r"C:\Users\test\Documents\report.txt",
                RuleScope::Files,
                &rules,
            ),
            MatchResult::Neutral
        );
    }

    #[test]
    fn applies_rules_only_to_their_selected_scope() {
        let rules = [Rule {
            scope: RuleScope::Folders,
            ..rule(1, "Temp", RuleType::Blacklist, true)
        }];

        assert_eq!(
            classify(r"C:\Temp", RuleScope::Files, &rules),
            MatchResult::Neutral
        );
        assert!(matches!(
            classify(r"C:\Temp", RuleScope::Folders, &rules),
            MatchResult::Targeted { .. }
        ));
    }

    fn rule(id: i64, keyword: &str, rule_type: RuleType, enabled: bool) -> Rule {
        Rule {
            id,
            keyword: keyword.to_string(),
            rule_type,
            scope: RuleScope::All,
            enabled,
            created_at: "2026-07-13 00:00:00".to_string(),
        }
    }
}
