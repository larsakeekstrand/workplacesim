//! Agent → room classifier. Mirrors `public/main.js` `isLabAgent` +
//! `classify`. Priority: lab keyword > plan-mode > desk.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    Desk,
    Meeting,
    Lab,
}

/// Keywords that route a sim to the lab. The JS match is substring-based,
/// case-insensitive, against `agent_type + " " + description`.
pub const LAB_KEYWORDS: &[&str] = &[
    "test", "spec", "review", "verify", "verifier", "lint", "bench", "analyzer", "hunter", "qa",
];

fn is_lab(agent_type: &str, description: &str) -> bool {
    // JS: `${agent_type || ""} ${description || ""}`.toLowerCase()
    let haystack = format!("{} {}", agent_type, description).to_lowercase();
    LAB_KEYWORDS.iter().any(|k| haystack.contains(k))
}

/// Classify an agent to a room. Priority matches CLAUDE.md and JS:
/// 1. Lab keyword in agent_type or description.
/// 2. `permission_mode == "plan"`.
/// 3. Desk.
pub fn classify(agent_type: &str, description: &str, permission_mode: &str) -> Room {
    if is_lab(agent_type, description) {
        Room::Lab
    } else if permission_mode == "plan" {
        Room::Meeting
    } else {
        Room::Desk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_keyword_triggers_lab() {
        for &k in LAB_KEYWORDS {
            assert_eq!(classify(k, "", ""), Room::Lab, "agent_type {k}");
            assert_eq!(classify("", k, ""), Room::Lab, "description {k}");
        }
    }

    #[test]
    fn substring_match_in_longer_text() {
        assert_eq!(classify("code-reviewer", "", ""), Room::Lab);
        assert_eq!(classify("", "runs the test suite", ""), Room::Lab);
        assert_eq!(classify("", "quick lint pass", ""), Room::Lab);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(classify("QA", "", ""), Room::Lab);
        assert_eq!(classify("Qa", "", ""), Room::Lab);
        assert_eq!(classify("qa", "", ""), Room::Lab);
        assert_eq!(classify("TESTER", "", ""), Room::Lab);
        assert_eq!(classify("", "REVIEW me", ""), Room::Lab);
    }

    #[test]
    fn plan_mode_routes_to_meeting() {
        assert_eq!(classify("helper", "", "plan"), Room::Meeting);
        assert_eq!(classify("", "", "plan"), Room::Meeting);
    }

    #[test]
    fn default_is_desk() {
        assert_eq!(classify("", "", ""), Room::Desk);
        assert_eq!(classify("claude", "", ""), Room::Desk);
        assert_eq!(classify("claude", "", "acceptEdits"), Room::Desk);
    }

    #[test]
    fn lab_keyword_wins_over_plan_mode() {
        assert_eq!(classify("reviewer", "", "plan"), Room::Lab);
        assert_eq!(classify("", "test harness", "plan"), Room::Lab);
    }

    #[test]
    fn non_keyword_words_do_not_trigger() {
        // "plan" is not a lab keyword despite being a valid permission mode.
        assert_eq!(classify("planner", "", ""), Room::Desk);
        assert_eq!(classify("", "just thinking", ""), Room::Desk);
    }
}
