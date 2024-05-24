use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use ring::digest;

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
    WeightedLeastConnections,
}

pub struct LoadBalancer {
    targets: HashMap<String, Vec<String>>,
    counters: HashMap<String, usize>,
    connections: HashMap<String, usize>,
    weights: HashMap<String, Vec<usize>>,
    weights_sum: HashMap<String, usize>,
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

        let mut weights_sum = HashMap::new();
        for (key, weight_list) in &weights {
            weights_sum.insert(key.clone(), weight_list.iter().sum());
        }

        LoadBalancer {
            targets,
            counters: HashMap::new(),
            connections: HashMap::new(),
            weights,
            weights_sum,
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
                        let sum_of_weights = *self.weights_sum.get(key)?;
                        let mut cumulative_weight = 0;
                        for (i, weight) in weight_list.iter().enumerate() {
                            cumulative_weight += weight;
                            if *counter < cumulative_weight {
                                *counter = (*counter + 1) % sum_of_weights;
                                return Some(target_list[i].clone());
                            }
                        }
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
                    LoadBalancingAlgorithm::WeightedLeastConnections => {
                        let weight_list = self.weights.get(key)?;
                        let target = target_list.iter().enumerate()
                            .map(|(i, t)| (t, self.connections.get(t).cloned().unwrap_or(0), weight_list[i]))
                            .min_by_key(|(_, c, w)| (*c as f64 / *w as f64) as usize)?;
                        *self.connections.entry(target.0.clone()).or_insert(0) += 1;
                        return Some(target.0.clone());
                    },
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn setup_targets() -> HashMap<String, Vec<String>> {
        let mut targets = HashMap::new();
        targets.insert("/service1".to_string(), vec!["http://service1-a".to_string(), "http://service1-b".to_string()]);
        targets.insert("/service2".to_string(), vec!["http://service2-a".to_string(), "http://service2-b".to_string()]);
        targets
    }

    #[test]
    fn test_round_robin() {
        let targets = setup_targets();
        let mut lb = LoadBalancer::new(targets, LoadBalancingAlgorithm::RoundRobin, None);

        assert_eq!(lb.get_target("/service1", None), Some("http://service1-a".to_string()));
        assert_eq!(lb.get_target("/service1", None), Some("http://service1-b".to_string()));
        assert_eq!(lb.get_target("/service1", None), Some("http://service1-a".to_string()));
    }

    #[test]
    fn test_random() {
        let targets = setup_targets();
        let mut lb = LoadBalancer::new(targets, LoadBalancingAlgorithm::Random, None);

        let target = lb.get_target("/service1", None);
        assert!(target == Some("http://service1-a".to_string()) || target == Some("http://service1-b".to_string()));
    }

    #[test]
    fn test_least_connections() {
        let targets = setup_targets();
        let mut lb = LoadBalancer::new(targets, LoadBalancingAlgorithm::LeastConnections, None);

        let target1 = lb.get_target("/service1", None);
        let target2 = lb.get_target("/service1", None);

        lb.connections.insert(target1.clone().unwrap(), 1);
        lb.connections.insert(target2.clone().unwrap(), 2);

        let target3 = lb.get_target("/service1", None);
        assert_eq!(target3, target1);
    }

    #[test]
    fn test_weighted_round_robin() {
        let targets = setup_targets();
        let mut weights = HashMap::new();
        weights.insert("/service1".to_string(), vec![2, 1]);

        let mut lb = LoadBalancer::new(targets, LoadBalancingAlgorithm::WeightedRoundRobin, Some(weights));

        assert_eq!(lb.get_target("/service1", None), Some("http://service1-a".to_string()));
        assert_eq!(lb.get_target("/service1", None), Some("http://service1-a".to_string()));
        assert_eq!(lb.get_target("/service1", None), Some("http://service1-b".to_string()));
        assert_eq!(lb.get_target("/service1", None), Some("http://service1-a".to_string()));
    }

    #[test]
    fn test_ip_hash() {
        let targets = setup_targets();
        let mut lb = LoadBalancer::new(targets, LoadBalancingAlgorithm::IPHash, None);

        let target1 = lb.get_target("/service1", Some("192.168.0.1"));
        let target2 = lb.get_target("/service1", Some("192.168.0.1"));

        assert_eq!(target1, target2);
    }

    #[test]
    fn test_consistent_hashing() {
        let targets = setup_targets();
        let mut lb = LoadBalancer::new(targets, LoadBalancingAlgorithm::ConsistentHashing, None);

        let target1 = lb.get_target("/service1", Some("192.168.0.1"));
        let target2 = lb.get_target("/service1", Some("192.168.0.1"));

        assert_eq!(target1, target2);
    }

    #[test]
    fn test_weighted_least_connections() {
        let targets = setup_targets();
        let mut weights = HashMap::new();
        weights.insert("/service1".to_string(), vec![3, 1]);

        let mut lb = LoadBalancer::new(targets, LoadBalancingAlgorithm::WeightedLeastConnections, Some(weights));

        let target1 = lb.get_target("/service1", None);
        let target2 = lb.get_target("/service1", None);

        lb.connections.insert(target1.clone().unwrap(), 1);
        lb.connections.insert(target2.clone().unwrap(), 2);

        let target3 = lb.get_target("/service1", None);
        assert_eq!(target3, target1);
    }
}

