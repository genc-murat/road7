use serde::{Deserialize, Serialize};
use hyper::Request;
use regex::Regex;
use tracing::info;

const USER_AGENT_LOG: &str = "User-Agent: {}";
const ALLOWED_USER_AGENT_LOG: &str = "Allowed User-Agent: {}";
const DENIED_USER_AGENT_LOG: &str = "Denied User-Agent: {}";
const PATTERN_MATCHED_USER_AGENT_LOG: &str = "Pattern Matched User-Agent: {}";
const NO_USER_AGENT_LOG: &str = "No User-Agent header present";
const EMPTY_USER_AGENT_LOG: &str = "Empty User-Agent detected";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BotDetectorConfig {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub patterns: Vec<String>,
    pub empty_user_agent_is_bot: bool,
}

pub async fn is_bot_request(req: &Request<hyper::Body>, config: &BotDetectorConfig) -> bool {
    if let Some(user_agent) = req.headers().get(hyper::header::USER_AGENT) {
        let user_agent = user_agent.to_str().unwrap_or_default();
        info!(USER_AGENT_LOG, user_agent);

        if config.allow.iter().any(|s| user_agent.contains(s)) {
            info!(ALLOWED_USER_AGENT_LOG, user_agent);
            return false;
        }

        if config.deny.iter().any(|s| user_agent.contains(s)) {
            info!(DENIED_USER_AGENT_LOG, user_agent);
            return true;
        }

        for pattern in &config.patterns {
            if let Ok(regex) = Regex::new(pattern) {
                if regex.is_match(user_agent) {
                    info!(PATTERN_MATCHED_USER_AGENT_LOG, user_agent);
                    return true;
                }
            }
        }
    } else {
        info!(NO_USER_AGENT_LOG);
    }

    if config.empty_user_agent_is_bot && req.headers().get(hyper::header::USER_AGENT).is_none() {
        info!(EMPTY_USER_AGENT_LOG);
        return true;
    }

    false
}
