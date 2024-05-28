use serde::{Deserialize, Serialize};
use hyper::Request;
use regex::Regex;
use tracing::info;
use std::collections::HashSet;

const USER_AGENT_LOG: &str = "User-Agent: {}";
const ALLOWED_USER_AGENT_LOG: &str = "Allowed User-Agent: {}";
const DENIED_USER_AGENT_LOG: &str = "Denied User-Agent: {}";
const PATTERN_MATCHED_USER_AGENT_LOG: &str = "Pattern Matched User-Agent: {}";
const NO_USER_AGENT_LOG: &str = "No User-Agent header present";
const EMPTY_USER_AGENT_LOG: &str = "Empty User-Agent detected";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BotDetectorConfig {
    pub allow: HashSet<String>,
    pub deny: HashSet<String>,
    pub patterns: Vec<String>,
    pub empty_user_agent_is_bot: bool,
}

pub async fn is_bot_request(req: &Request<hyper::Body>, config: &BotDetectorConfig) -> bool {
    match req.headers().get(hyper::header::USER_AGENT) {
        Some(user_agent) => match user_agent.to_str() {
            Ok(user_agent) => {
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
            }
            Err(_) => {
                info!(USER_AGENT_LOG, "Invalid User-Agent header");
            }
        },
        None => {
            info!(NO_USER_AGENT_LOG);
            if config.empty_user_agent_is_bot {
                info!(EMPTY_USER_AGENT_LOG);
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::USER_AGENT;
    use hyper::Body;
    use hyper::Request;

    fn create_config(allow: Vec<&str>, deny: Vec<&str>, patterns: Vec<&str>, empty_user_agent_is_bot: bool) -> BotDetectorConfig {
        BotDetectorConfig {
            allow: allow.into_iter().map(String::from).collect(),
            deny: deny.into_iter().map(String::from).collect(),
            patterns: patterns.into_iter().map(String::from).collect(),
            empty_user_agent_is_bot,
        }
    }

    #[tokio::test]
    async fn test_allow_list() {
        let config = create_config(vec!["Mozilla"], vec![], vec![], false);

        let req = Request::builder()
            .header(USER_AGENT, "Mozilla/5.0")
            .body(Body::empty())
            .unwrap();

        assert_eq!(is_bot_request(&req, &config).await, false);
    }

    #[tokio::test]
    async fn test_deny_list() {
        let config = create_config(vec![], vec!["BadBot"], vec![], false);

        let req = Request::builder()
            .header(USER_AGENT, "BadBot/1.0")
            .body(Body::empty())
            .unwrap();

        assert_eq!(is_bot_request(&req, &config).await, true);
    }

    #[tokio::test]
    async fn test_patterns_list() {
        let config = create_config(vec![], vec![], vec![r"Bot/.*"], false);

        let req = Request::builder()
            .header(USER_AGENT, "SomeBot/1.0")
            .body(Body::empty())
            .unwrap();

        assert_eq!(is_bot_request(&req, &config).await, true);
    }

    #[tokio::test]
    async fn test_empty_user_agent_is_bot() {
        let config = create_config(vec![], vec![], vec![], true);

        let req = Request::builder()
            .body(Body::empty())
            .unwrap();

        assert_eq!(is_bot_request(&req, &config).await, true);
    }

    #[tokio::test]
    async fn test_empty_user_agent_is_not_bot() {
        let config = create_config(vec![], vec![], vec![], false);

        let req = Request::builder()
            .body(Body::empty())
            .unwrap();

        assert_eq!(is_bot_request(&req, &config).await, false);
    }

    #[tokio::test]
    async fn test_no_user_agent_header() {
        let config = create_config(vec!["Mozilla"], vec!["BadBot"], vec![r"Bot/.*"], false);

        let req = Request::builder()
            .body(Body::empty())
            .unwrap();

        assert_eq!(is_bot_request(&req, &config).await, false);
    }
}
