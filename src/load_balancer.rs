use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use ring::digest;
use tokio::sync::Mutex;
use std::sync::Arc;

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

#[derive(Clone)]
pub struct LoadBalancer {
    targets: HashMap<String, Vec<String>>,
    counters: Arc<Mutex<HashMap<String, Arc<Mutex<usize>>>>>,
    connections: Arc<Mutex<HashMap<String, Arc<Mutex<usize>>>>>,
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
            counters: Arc::new(Mutex::new(HashMap::new())),
            connections: Arc::new(Mutex::new(HashMap::new())),
            weights,
            weights_sum,
            algorithm,
        }
    }

    pub async fn get_target(&self, path: &str, client_ip: Option<&str>) -> Option<String> {
        for (key, target_list) in &self.targets {
            if path.starts_with(key) {
                match self.algorithm {
                    LoadBalancingAlgorithm::RoundRobin => {
                        let mut counters = self.counters.lock().await;
                        let counter = counters.entry(key.to_string())
                                              .or_insert_with(|| Arc::new(Mutex::new(0)));
                        let target_list = target_list.clone();
                        let mut counter_guard = counter.lock().await;
                        let target = target_list[*counter_guard % target_list.len()].clone();
                        *counter_guard = (*counter_guard + 1) % target_list.len();
                        return Some(target);
                    },
                    LoadBalancingAlgorithm::Random => {
                        let mut rng = thread_rng();
                        return target_list.choose(&mut rng).cloned();
                    },
                    LoadBalancingAlgorithm::LeastConnections => {
                        let min_target = {
                            let mut connections = self.connections.lock().await;
                            let target_connections: Vec<_> = target_list.iter()
                                .map(|t| {
                                    let conn_count = connections.entry(t.clone())
                                                                .or_insert_with(|| Arc::new(Mutex::new(0)));
                                    (t.clone(), conn_count.clone())
                                })
                                .collect();

                            let mut min_conn_count = usize::MAX;
                            let mut min_target = None;
                            for (t, conn_count) in target_connections {
                                let count = *conn_count.lock().await;
                                if count < min_conn_count {
                                    min_conn_count = count;
                                    min_target = Some((t, conn_count));
                                }
                            }
                            min_target
                        };

                        if let Some((t, conn_count)) = min_target {
                            *conn_count.lock().await += 1;
                            return Some(t);
                        }
                    },
                    LoadBalancingAlgorithm::WeightedRoundRobin => {
                        let mut counters = self.counters.lock().await;
                        let counter = counters.entry(key.to_string())
                                              .or_insert_with(|| Arc::new(Mutex::new(0)));
                        let weight_list = self.weights.get(key)?;
                        let sum_of_weights = *self.weights_sum.get(key)?;
                        let mut counter_guard = counter.lock().await;
                        let mut cumulative_weight = 0;
                        for (i, weight) in weight_list.iter().enumerate() {
                            cumulative_weight += weight;
                            if *counter_guard < cumulative_weight {
                                *counter_guard = (*counter_guard + 1) % sum_of_weights;
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
                            let index = (u64::from_le_bytes(hash.as_ref()[..8].try_into().unwrap_or_default()) % target_list.len() as u64) as usize;
                            return Some(target_list[index].clone());
                        }
                    },
                    LoadBalancingAlgorithm::WeightedLeastConnections => {
                        let min_target = {
                            let mut connections = self.connections.lock().await;
                            let weight_list = self.weights.get(key)?;
                            let target_connections: Vec<_> = target_list.iter().enumerate()
                                .map(|(i, t)| {
                                    let conn_count = connections.entry(t.clone())
                                                                .or_insert_with(|| Arc::new(Mutex::new(0)));
                                    (t.clone(), conn_count.clone(), weight_list[i])
                                })
                                .collect();

                            let mut min_score = f64::MAX;
                            let mut min_target = None;
                            for (t, conn_count, weight) in target_connections {
                                let count = *conn_count.lock().await;
                                let score = count as f64 / weight as f64;
                                if score < min_score {
                                    min_score = score;
                                    min_target = Some((t, conn_count));
                                }
                            }
                            min_target
                        };

                        if let Some((t, conn_count)) = min_target {
                            *conn_count.lock().await += 1;
                            return Some(t);
                        }
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

    async fn create_test_load_balancer(algorithm: LoadBalancingAlgorithm) -> LoadBalancer {
        let targets = HashMap::from([
            ("/api".to_string(), vec!["http://localhost:8080".to_string(), "http://localhost:8081".to_string()]),
        ]);
        let weights = Some(HashMap::from([
            ("/api".to_string(), vec![3, 1]),
        ]));
        LoadBalancer::new(targets, algorithm, weights)
    }

    #[tokio::test]
    async fn test_round_robin() {
        let lb = create_test_load_balancer(LoadBalancingAlgorithm::RoundRobin).await;

        let target1 = lb.get_target("/api/resource", None).await.unwrap();
        let target2 = lb.get_target("/api/resource", None).await.unwrap();
        let target3 = lb.get_target("/api/resource", None).await.unwrap();

        assert_eq!(target1, "http://localhost:8080");
        assert_eq!(target2, "http://localhost:8081");
        assert_eq!(target3, "http://localhost:8080");
    }

    #[tokio::test]
    async fn test_random() {
        let lb = create_test_load_balancer(LoadBalancingAlgorithm::Random).await;

        let target1 = lb.get_target("/api/resource", None).await.unwrap();
        let target2 = lb.get_target("/api/resource", None).await.unwrap();

        assert!(["http://localhost:8080", "http://localhost:8081"].contains(&target1.as_str()));
        assert!(["http://localhost:8080", "http://localhost:8081"].contains(&target2.as_str()));
    }

    #[tokio::test]
    async fn test_least_connections() {
        let lb = create_test_load_balancer(LoadBalancingAlgorithm::LeastConnections).await;

        let target1 = lb.get_target("/api/resource", None).await.unwrap();
        let target2 = lb.get_target("/api/resource", None).await.unwrap();

        assert_eq!(target1, "http://localhost:8080");
        assert_eq!(target2, "http://localhost:8081");
    }

    #[tokio::test]
    async fn test_weighted_round_robin() {
        let lb = create_test_load_balancer(LoadBalancingAlgorithm::WeightedRoundRobin).await;

        let target1 = lb.get_target("/api/resource", None).await.unwrap();
        let target2 = lb.get_target("/api/resource", None).await.unwrap();
        let target3 = lb.get_target("/api/resource", None).await.unwrap();
        let target4 = lb.get_target("/api/resource", None).await.unwrap();

        assert_eq!(target1, "http://localhost:8080");
        assert_eq!(target2, "http://localhost:8080");
        assert_eq!(target3, "http://localhost:8080");
        assert_eq!(target4, "http://localhost:8081");
    }

    #[tokio::test]
    async fn test_ip_hash() {
        let lb = create_test_load_balancer(LoadBalancingAlgorithm::IPHash).await;

        let target1 = lb.get_target("/api/resource", Some("192.168.1.1")).await.unwrap();
        let target2 = lb.get_target("/api/resource", Some("192.168.1.2")).await.unwrap();

        assert!(["http://localhost:8080", "http://localhost:8081"].contains(&target1.as_str()));
        assert!(["http://localhost:8080", "http://localhost:8081"].contains(&target2.as_str()));
        assert_ne!(target1, target2);
    }

    #[tokio::test]
    async fn test_consistent_hashing() {
        let lb = create_test_load_balancer(LoadBalancingAlgorithm::ConsistentHashing).await;

        let target1 = lb.get_target("/api/resource", Some("192.168.1.1")).await.unwrap();
        let target2 = lb.get_target("/api/resource", Some("192.168.1.2")).await.unwrap();

        assert!(["http://localhost:8080", "http://localhost:8081"].contains(&target1.as_str()));
        assert!(["http://localhost:8080", "http://localhost:8081"].contains(&target2.as_str()));
        assert_ne!(target1, target2);
    }

    #[tokio::test]
    async fn test_weighted_least_connections() {
        let lb = create_test_load_balancer(LoadBalancingAlgorithm::WeightedLeastConnections).await;

        let target1 = lb.get_target("/api/resource", None).await.unwrap();
        let target2 = lb.get_target("/api/resource", None).await.unwrap();

        // Ensure that the least connected target is selected considering weights
        // http://localhost:8080 should be selected 3 times more often than http://localhost:8081
        let mut count_8080 = 0;
        let mut count_8081 = 0;

        for _ in 0..100 {
            let target = lb.get_target("/api/resource", None).await.unwrap();
            if target == "http://localhost:8080" {
                count_8080 += 1;
            } else if target == "http://localhost:8081" {
                count_8081 += 1;
            }
        }

        assert!(count_8080 > count_8081 * 3 / 4);
    }
}

