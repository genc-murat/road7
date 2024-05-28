use std::time::Duration;
use rand::{Rng, thread_rng};
use serde::{Deserialize, Serialize};

pub trait RetryStrategy: Send + Sync {
    fn delay(&mut self) -> Duration;
    fn max_attempts(&self) -> usize;
}

pub struct FixedIntervalBackoffStrategy {
    base_delay: Duration,
    max_attempts: usize,
}

impl FixedIntervalBackoffStrategy {
    pub fn new(base_delay: Duration, max_attempts: usize) -> Self {
        Self {
            base_delay,
            max_attempts,
        }
    }
}

impl RetryStrategy for FixedIntervalBackoffStrategy {
    fn delay(&mut self) -> Duration {
        self.base_delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

pub struct ExponentialBackoffStrategy {
    base_delay: Duration,
    current_attempt: usize,
    factor: f64,
    max_attempts: usize,
}

impl ExponentialBackoffStrategy {
    pub fn new(base_delay: Duration, factor: f64, max_attempts: usize) -> Self {
        Self {
            base_delay,
            current_attempt: 1,
            factor,
            max_attempts,
        }
    }
}

impl RetryStrategy for ExponentialBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay = self.base_delay.mul_f64(self.factor.powf(self.current_attempt as f64));
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

pub struct LinearBackoffStrategy {
    base_delay: Duration,
    step: Duration,
    current_attempt: usize,
    max_attempts: usize,
}

impl LinearBackoffStrategy {
    pub fn new(base_delay: Duration, step: Duration, max_attempts: usize) -> Self {
        Self {
            base_delay,
            step,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for LinearBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay = self.base_delay + self.step * (self.current_attempt as u32);
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

pub struct RandomDelayStrategy {
    min_delay: Duration,
    max_delay: Duration,
    max_attempts: usize,
}

impl RandomDelayStrategy {
    pub fn new(min_delay: Duration, max_delay: Duration, max_attempts: usize) -> Self {
        Self {
            min_delay,
            max_delay,
            max_attempts,
        }
    }
}

impl RetryStrategy for RandomDelayStrategy {
    fn delay(&mut self) -> Duration {
        let mut rng = thread_rng();
        let secs = rng.gen_range(self.min_delay.as_secs()..self.max_delay.as_secs());
        let nanos = rng.gen_range(0..1_000_000_000);
        Duration::new(secs, nanos as u32)
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

pub struct IncrementalBackoffStrategy {
    initial_delay: Duration,
    increment: Duration,
    current_attempt: usize,
    max_attempts: usize,
}

impl IncrementalBackoffStrategy {
    pub fn new(initial_delay: Duration, increment: Duration, max_attempts: usize) -> Self {
        Self {
            initial_delay,
            increment,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for IncrementalBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay = self.initial_delay + self.increment * (self.current_attempt as u32);
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

pub struct FibonacciBackoffStrategy {
    base_delay: Duration,
    current: u64,
    next: u64,
    max_attempts: usize,
}

impl FibonacciBackoffStrategy {
    pub fn new(base_delay: Duration, max_attempts: usize) -> Self {
        Self {
            base_delay,
            current: 1,
            next: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for FibonacciBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay_seconds = self.base_delay.as_secs() * self.current;
        let new_next = self.current + self.next;
        self.current = self.next;
        self.next = new_next;

        Duration::from_secs(delay_seconds)
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

pub struct GeometricBackoffStrategy {
    base_delay: Duration,
    factor: f64,
    current_attempt: usize,
    max_attempts: usize,
}

impl GeometricBackoffStrategy {
    pub fn new(base_delay: Duration, factor: f64, max_attempts: usize) -> Self {
        Self {
            base_delay,
            factor,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for GeometricBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay = self.base_delay.mul_f64(self.factor.powf(self.current_attempt as f64));
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

pub struct HarmonicBackoffStrategy {
    base_delay: Duration,
    current_attempt: usize,
    max_attempts: usize,
}

impl HarmonicBackoffStrategy {
    pub fn new(base_delay: Duration, max_attempts: usize) -> Self {
        Self {
            base_delay,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for HarmonicBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let divisor = self.current_attempt as u32;
        let total_nanos = self.base_delay.as_secs() * 1_000_000_000 + self.base_delay.subsec_nanos() as u64;
        let nanos_per_attempt = total_nanos / divisor as u64;
        let delay = Duration::new(nanos_per_attempt / 1_000_000_000, (nanos_per_attempt % 1_000_000_000) as u32);
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

pub struct JitterBackoffStrategy {
    strategy: Box<dyn RetryStrategy>,
    jitter_factor: f64,
    max_attempts: usize,
}

impl JitterBackoffStrategy {
    pub fn new(strategy: Box<dyn RetryStrategy>, jitter_factor: f64, max_attempts: usize) -> Self {
        Self {
            strategy,
            jitter_factor,
            max_attempts,
        }
    }
}

impl RetryStrategy for JitterBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let base_delay = self.strategy.delay();
        let jitter = base_delay.mul_f64(self.jitter_factor);
        let mut rng = thread_rng();
        let secs = rng.gen_range(0..jitter.as_secs());
        let nanos = rng.gen_range(0..1_000_000_000);
        base_delay + Duration::new(secs, nanos as u32)
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RetryConfig {
    pub strategy: String,
    pub base_delay_seconds: u64,
    pub max_attempts: usize,
    pub factor: Option<f64>,
    pub step_delay_seconds: Option<u64>,
}

impl RetryConfig {
    pub fn to_strategy(&self) -> Box<dyn RetryStrategy> {
        match self.strategy.as_str() {
            "ExponentialBackoff" => Box::new(ExponentialBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.factor.unwrap_or(2.0),
                self.max_attempts,
            )),
            "LinearBackoff" => Box::new(LinearBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                Duration::from_secs(self.step_delay_seconds.unwrap_or(self.base_delay_seconds)),
                self.max_attempts,
            )),
            "FixedInterval" => Box::new(FixedIntervalBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.max_attempts,
            )),
            "RandomDelay" => Box::new(RandomDelayStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                Duration::from_secs(self.step_delay_seconds.unwrap_or(self.base_delay_seconds * 2)),
                self.max_attempts,
            )),
            "IncrementalBackoff" => Box::new(IncrementalBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                Duration::from_secs(self.step_delay_seconds.unwrap_or(1)),
                self.max_attempts,
            )),
            "FibonacciBackoff" => Box::new(FibonacciBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.max_attempts,
            )),
            "GeometricBackoff" => Box::new(GeometricBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.factor.unwrap_or(2.0),
                self.max_attempts,
            )),
            "HarmonicBackoff" => Box::new(HarmonicBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.max_attempts,
            )),
            "JitterBackoff" => Box::new(JitterBackoffStrategy::new(
                Box::new(ExponentialBackoffStrategy::new(
                    Duration::from_secs(self.base_delay_seconds),
                    self.factor.unwrap_or(2.0),
                    self.max_attempts
                )),
                0.5,
                self.max_attempts,
            )),
            _ => Box::new(FixedIntervalBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.max_attempts,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_fixed_interval_backoff() {
        let mut strategy = FixedIntervalBackoffStrategy::new(Duration::from_secs(2), 3);
        assert_eq!(strategy.delay(), Duration::from_secs(2));
        assert_eq!(strategy.delay(), Duration::from_secs(2));
        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_exponential_backoff() {
        let mut strategy = ExponentialBackoffStrategy::new(Duration::from_secs(1), 2.0, 3);
        assert_eq!(strategy.delay(), Duration::from_secs_f64(1.0 * 2f64.powf(1.0)));
        assert_eq!(strategy.delay(), Duration::from_secs_f64(1.0 * 2f64.powf(2.0)));
        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_linear_backoff() {
        let mut strategy = LinearBackoffStrategy::new(Duration::from_secs(1), Duration::from_secs(2), 3);
        assert_eq!(strategy.delay(), Duration::from_secs(3));
        assert_eq!(strategy.delay(), Duration::from_secs(5));
        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_random_delay_backoff() {
        let mut strategy = RandomDelayStrategy::new(Duration::from_secs(1), Duration::from_secs(5), 3);
        let delay = strategy.delay();
        assert!(delay >= Duration::from_secs(1) && delay <= Duration::from_secs(5));
        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_incremental_backoff() {
        let mut strategy = IncrementalBackoffStrategy::new(Duration::from_secs(1), Duration::from_secs(2), 3);
        assert_eq!(strategy.delay(), Duration::from_secs(3));
        assert_eq!(strategy.delay(), Duration::from_secs(5));
        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_fibonacci_backoff() {
        let mut strategy = FibonacciBackoffStrategy::new(Duration::from_secs(1), 3);
        assert_eq!(strategy.delay(), Duration::from_secs(1));
        assert_eq!(strategy.delay(), Duration::from_secs(1));
        assert_eq!(strategy.delay(), Duration::from_secs(2));
        assert_eq!(strategy.delay(), Duration::from_secs(3));
        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_geometric_backoff() {
        let mut strategy = GeometricBackoffStrategy::new(Duration::from_secs(1), 2.0, 3);
        assert_eq!(strategy.delay(), Duration::from_secs_f64(1.0 * 2f64.powf(1.0)));
        assert_eq!(strategy.delay(), Duration::from_secs_f64(1.0 * 2f64.powf(2.0)));
        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_harmonic_backoff() {
        let mut strategy = HarmonicBackoffStrategy::new(Duration::from_secs(10), 3);
        assert_eq!(strategy.delay(), Duration::from_secs(10));
        assert_eq!(strategy.delay(), Duration::from_secs(5)); 

        let expected_delay = Duration::from_secs_f64(10.0 / 3.0);
        let actual_delay = strategy.delay();
        let tolerance = Duration::from_millis(100);

        assert!(
            actual_delay >= expected_delay - tolerance && actual_delay <= expected_delay + tolerance,
            "Expected delay: {:?}, actual delay: {:?}",
            expected_delay,
            actual_delay
        );

        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_jitter_backoff() {
        let base_strategy = ExponentialBackoffStrategy::new(Duration::from_secs(1), 2.0, 3);
        let mut strategy = JitterBackoffStrategy::new(Box::new(base_strategy), 0.5, 3);
        let base_delay = strategy.strategy.delay();
        let delay = strategy.delay();
        assert!(delay >= base_delay);
        assert_eq!(strategy.max_attempts(), 3);
    }

    #[test]
    fn test_retry_config_to_strategy() {
        let config = RetryConfig {
            strategy: "ExponentialBackoff".to_string(),
            base_delay_seconds: 1,
            max_attempts: 3,
            factor: Some(2.0),
            step_delay_seconds: None,
        };
        let mut strategy = config.to_strategy();
        assert_eq!(strategy.delay(), Duration::from_secs_f64(1.0 * 2f64.powf(1.0)));
        assert_eq!(strategy.delay(), Duration::from_secs_f64(1.0 * 2f64.powf(2.0)));
        assert_eq!(strategy.max_attempts(), 3);
    }
}
