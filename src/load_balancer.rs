use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error;
use std::collections::HashMap;
use tracing::error;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoadBalancerConfig {
    pub algorithm: LoadBalancingAlgorithm,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum LoadBalancingAlgorithm {
    RoundRobin,
    Random,
}

pub struct LoadBalancer {
    targets: HashMap<String, Vec<String>>,
    counters: HashMap<String, usize>,
    algorithm: LoadBalancingAlgorithm,
}

impl LoadBalancer {
    pub fn new(targets: HashMap<String, Vec<String>>, algorithm: LoadBalancingAlgorithm) -> Self {
        LoadBalancer {
            targets,
            counters: HashMap::new(),
            algorithm,
        }
    }

    pub fn get_target(&mut self, path: &str) -> Option<String> {
        for (key, target_list) in &self.targets {
            if path.starts_with(key) {
                match self.algorithm {
                    LoadBalancingAlgorithm::RoundRobin => {
                        let counter = self.counters.entry(key.to_string()).or_insert(0);
                        let target = target_list[*counter % target_list.len()].clone();
                        *counter = (*counter + 1) % target_list.len();
                        return Some(target);
                    },
                    LoadBalancingAlgorithm::Random => {
                        let mut rng = thread_rng();
                        return target_list.choose(&mut rng).cloned();
                    }
                }
            }
        }
        None
    }
}