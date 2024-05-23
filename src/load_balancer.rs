use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use ring::digest;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoadBalancerConfig {
    pub algorithm: LoadBalancingAlgorithm,
    pub weights: Option<HashMap<String, Vec<usize>>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum LoadBalancingAlgorithm {
    RoundRobin,
    Random,
    LeastConnections,
    WeightedRoundRobin,
    IPHash,
    ConsistentHashing,
}

pub struct LoadBalancer {
    targets: HashMap<String, Vec<String>>,
    counters: HashMap<String, usize>,
    connections: HashMap<String, usize>,
    weights: HashMap<String, Vec<usize>>,
    algorithm: LoadBalancingAlgorithm,
}

impl LoadBalancer {
    pub fn new(
        targets: HashMap<String, Vec<String>>,
        algorithm: LoadBalancingAlgorithm,
        weights: Option<HashMap<String, Vec<usize>>>,
    ) -> Self {
        let weights = weights.unwrap_or_else(|| {
            let mut default_weights = HashMap::new();
            for (key, target_list) in &targets {
                default_weights.insert(key.clone(), vec![1; target_list.len()]);
            }
            default_weights
        });

        LoadBalancer {
            targets,
            counters: HashMap::new(),
            connections: HashMap::new(),
            weights,
            algorithm,
        }
    }

    pub fn get_target(&mut self, path: &str, client_ip: Option<&str>) -> Option<String> {
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
                    },
                    LoadBalancingAlgorithm::LeastConnections => {
                        let min_connection = target_list.iter()
                            .map(|t| (t, self.connections.get(t).cloned().unwrap_or(0)))
                            .min_by_key(|(_, c)| *c)?;
                        *self.connections.entry(min_connection.0.clone()).or_insert(0) += 1;
                        return Some(min_connection.0.clone());
                    },
                    LoadBalancingAlgorithm::WeightedRoundRobin => {
                        let counter = self.counters.entry(key.to_string()).or_insert(0);
                        let weight_list = self.weights.get(key)?;
                        let mut sum_of_weights: usize = 0;
                        let mut target = None;
                        for (i, weight) in weight_list.iter().enumerate() {
                            sum_of_weights += weight;
                            if *counter < sum_of_weights {
                                target = Some(target_list[i].clone());
                                break;
                            }
                        }
                        *counter = (*counter + 1) % sum_of_weights;
                        return target;
                    },
                    LoadBalancingAlgorithm::IPHash => {
                        if let Some(ip) = client_ip {
                            let mut hasher = DefaultHasher::new();
                            ip.hash(&mut hasher);
                            let hash = hasher.finish();
                            let index = (hash % target_list.len() as u64) as usize;
                            return Some(target_list[index].clone());
                        }
                    },
                    LoadBalancingAlgorithm::ConsistentHashing => {
                        if let Some(ip) = client_ip {
                            let mut context = digest::Context::new(&digest::SHA256);
                            context.update(ip.as_bytes());
                            let hash = context.finish();
                            let index = (u64::from_le_bytes(hash.as_ref()[..8].try_into().unwrap()) % target_list.len() as u64) as usize;
                            return Some(target_list[index].clone());
                        }
                    },
                }
            }
        }
        None
    }
}