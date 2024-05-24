use serde::{Deserialize, Serialize};
use hyper::Request;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BotDetectorConfig {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub patterns: Vec<String>,
    pub empty_user_agent_is_bot: bool,
}

pub fn is_bot_request(req: &Request<hyper::Body>, config: &BotDetectorConfig) -> bool {
    if let Some(user_agent) = req.headers().get(hyper::header::USER_AGENT) {
        let user_agent = user_agent.to_str().unwrap_or_default();
        if config.allow.iter().any(|s| user_agent.contains(s)) {
            return false;
        }
    }

    if let Some(user_agent) = req.headers().get(hyper::header::USER_AGENT) {
        let user_agent = user_agent.to_str().unwrap_or_default();
        if config.deny.iter().any(|s| user_agent.contains(s)) {
            return true;
        }
    }

    if let Some(user_agent) = req.headers().get(hyper::header::USER_AGENT) {
        let user_agent = user_agent.to_str().unwrap_or_default();
        for pattern in &config.patterns {
            if regex::Regex::new(pattern).unwrap().is_match(user_agent) {
                return true;
            }
        }
    }

    if config.empty_user_agent_is_bot && req.headers().get(hyper::header::USER_AGENT).is_none() {
        return true;
    }

    false
}