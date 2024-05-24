use serde::{Deserialize, Serialize};
use hyper::Request;
use regex::Regex;

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
        println!("User-Agent: {}", user_agent);

        // Check if user agent is in the allow list
        if config.allow.iter().any(|s| user_agent.contains(s)) {
            println!("Allowed User-Agent: {}", user_agent);
            return false;
        }

        // Check if user agent is in the deny list
        if config.deny.iter().any(|s| user_agent.contains(s)) {
            println!("Denied User-Agent: {}", user_agent);
            return true;
        }

        // Check if user agent matches any pattern
        for pattern in &config.patterns {
            if let Ok(regex) = Regex::new(pattern) {
                if regex.is_match(user_agent) {
                    println!("Pattern Matched User-Agent: {}", user_agent);
                    return true;
                }
            }
        }
    } else {
        println!("No User-Agent header present");
    }

    // Check if user agent is empty and empty_user_agent_is_bot is true
    if config.empty_user_agent_is_bot && req.headers().get(hyper::header::USER_AGENT).is_none() {
        println!("Empty User-Agent detected");
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Body, Request};

    fn create_request_with_user_agent(user_agent: Option<&str>) -> Request<Body> {
        let mut req = Request::builder();
        if let Some(ua) = user_agent {
            req = req.header(hyper::header::USER_AGENT, ua);
        }
        req.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn test_allow_list() {
        let config = BotDetectorConfig {
            allow: vec!["Mozilla".into()],
            deny: vec!["Bot".into()],
            patterns: vec![],
            empty_user_agent_is_bot: false,
        };

        let req = create_request_with_user_agent(Some("Mozilla/5.0"));
        assert!(!is_bot_request(&req, &config), "Mozilla/5.0 should be allowed");

        let req = create_request_with_user_agent(Some("Bot/1.0"));
        assert!(is_bot_request(&req, &config), "Bot/1.0 should be denied");
    }

    #[tokio::test]
    async fn test_deny_list() {
        let config = BotDetectorConfig {
            allow: vec![],
            deny: vec!["Bot".into()],
            patterns: vec![],
            empty_user_agent_is_bot: false,
        };

        let req = create_request_with_user_agent(Some("Mozilla/5.0"));
        assert!(!is_bot_request(&req, &config), "Mozilla/5.0 should be allowed");

        let req = create_request_with_user_agent(Some("Bot/1.0"));
        assert!(is_bot_request(&req, &config), "Bot/1.0 should be denied");
    }

    #[tokio::test]
    async fn test_patterns() {
        let config = BotDetectorConfig {
            allow: vec![],
            deny: vec![],
            patterns: vec!["Bot.*".into()],
            empty_user_agent_is_bot: false,
        };

        let req = create_request_with_user_agent(Some("Mozilla/5.0"));
        assert!(!is_bot_request(&req, &config), "Mozilla/5.0 should be allowed");

        let req = create_request_with_user_agent(Some("Bot/1.0"));
        assert!(is_bot_request(&req, &config), "Bot/1.0 should be denied");

        let req = create_request_with_user_agent(Some("BotScanner/2.0"));
        assert!(is_bot_request(&req, &config), "BotScanner/2.0 should be denied");
    }

    #[tokio::test]
    async fn test_empty_user_agent() {
        let config = BotDetectorConfig {
            allow: vec![],
            deny: vec![],
            patterns: vec![],
            empty_user_agent_is_bot: true,
        };

        let req = create_request_with_user_agent(None);
        assert!(is_bot_request(&req, &config), "Empty user agent should be denied");

        let req = create_request_with_user_agent(Some("Mozilla/5.0"));
        assert!(!is_bot_request(&req, &config), "Mozilla/5.0 should be allowed");
    }

    #[tokio::test]
    async fn test_combined_config() {
        let config = BotDetectorConfig {
            allow: vec!["Mozilla".into()],
            deny: vec!["EvilBot".into()],
            patterns: vec!["BadBot.*".into()],
            empty_user_agent_is_bot: true,
        };

        let req = create_request_with_user_agent(Some("Mozilla/5.0"));
        assert!(!is_bot_request(&req, &config), "Mozilla/5.0 should be allowed");

        let req = create_request_with_user_agent(Some("EvilBot/1.0"));
        assert!(is_bot_request(&req, &config), "EvilBot/1.0 should be denied");

        let req = create_request_with_user_agent(Some("BadBotScanner/2.0"));
        assert!(is_bot_request(&req, &config), "BadBotScanner/2.0 should be denied");

        let req = create_request_with_user_agent(None);
        assert!(is_bot_request(&req, &config), "Empty user agent should be denied");
    }
}
