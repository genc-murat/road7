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
