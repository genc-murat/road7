use prometheus::{
    Encoder, Histogram, HistogramOpts, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry, TextEncoder,
};

#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,
    pub http_requests_total: IntCounter,
    pub ongoing_requests: IntGauge,
    pub http_responses_total: IntCounter,
    pub successful_requests: IntCounter,
    pub failed_requests: IntCounter,
    pub request_duration_seconds: Histogram,
    pub request_size_bytes: Histogram,
    pub response_size_bytes: Histogram,
    pub error_counts: IntCounterVec,
    pub http_method_counts: IntCounterVec,
    pub status_code_counts: IntCounterVec,
    pub retry_counts: IntCounter,
    pub circuit_breaker_states: IntGaugeVec,
    pub rate_limiter_hits: IntCounter,
    pub cache_hits: IntCounter,
    pub cache_misses: IntCounter,
    pub transform_counts: IntCounterVec,
    pub authentication_attempts: IntCounterVec,
    pub timeout_counts: IntCounter,
    pub connection_errors: IntCounter,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let http_requests_total_opts = Opts::new("http_requests_total", "Total number of HTTP requests received");
        let http_requests_total = IntCounter::with_opts(http_requests_total_opts).unwrap();
        registry.register(Box::new(http_requests_total.clone())).unwrap();

        let ongoing_requests_opts = Opts::new("ongoing_requests", "Number of ongoing requests");
        let ongoing_requests = IntGauge::with_opts(ongoing_requests_opts).unwrap();
        registry.register(Box::new(ongoing_requests.clone())).unwrap();

        let http_responses_total_opts = Opts::new("http_responses_total", "Total number of HTTP responses sent");
        let http_responses_total = IntCounter::with_opts(http_responses_total_opts).unwrap();
        registry.register(Box::new(http_responses_total.clone())).unwrap();

        let successful_requests_opts = Opts::new("successful_requests", "Total number of successful requests");
        let successful_requests = IntCounter::with_opts(successful_requests_opts).unwrap();
        registry.register(Box::new(successful_requests.clone())).unwrap();

        let failed_requests_opts = Opts::new("failed_requests", "Total number of failed requests");
        let failed_requests = IntCounter::with_opts(failed_requests_opts).unwrap();
        registry.register(Box::new(failed_requests.clone())).unwrap();

        let request_duration_seconds_opts = HistogramOpts::new("request_duration_seconds", "Request duration in seconds");
        let request_duration_seconds = Histogram::with_opts(request_duration_seconds_opts).unwrap();
        registry.register(Box::new(request_duration_seconds.clone())).unwrap();

        let request_size_bytes_opts = HistogramOpts::new("request_size_bytes", "Size of HTTP requests in bytes");
        let request_size_bytes = Histogram::with_opts(request_size_bytes_opts).unwrap();
        registry.register(Box::new(request_size_bytes.clone())).unwrap();

        let response_size_bytes_opts = HistogramOpts::new("response_size_bytes", "Size of HTTP responses in bytes");
        let response_size_bytes = Histogram::with_opts(response_size_bytes_opts).unwrap();
        registry.register(Box::new(response_size_bytes.clone())).unwrap();

        let error_counts_opts = Opts::new("error_counts", "Counts of different types of errors")
            .variable_labels(vec!["error_type".to_string()]);
        let error_counts = IntCounterVec::new(error_counts_opts, &["error_type"]).unwrap();
        error_counts.with_label_values(&["init"]).inc(); // Initialize with a label value
        registry.register(Box::new(error_counts.clone())).unwrap();

        let http_method_counts = IntCounterVec::new(
            Opts::new("http_method_counts", "Number of HTTP requests by method"),
            &["method"],
        ).unwrap();
        http_method_counts.with_label_values(&["init"]).inc(); // Initialize with a label value
        registry.register(Box::new(http_method_counts.clone())).unwrap();

        let status_code_counts = IntCounterVec::new(
            Opts::new("status_code_counts", "Number of HTTP responses by status code"),
            &["status_code"],
        ).unwrap();
        status_code_counts.with_label_values(&["init"]).inc(); // Initialize with a label value
        registry.register(Box::new(status_code_counts.clone())).unwrap();

        let retry_counts_opts = Opts::new("retry_counts", "Total number of retries attempted");
        let retry_counts = IntCounter::with_opts(retry_counts_opts).unwrap();
        registry.register(Box::new(retry_counts.clone())).unwrap();

        let circuit_breaker_states = IntGaugeVec::new(
            Opts::new("circuit_breaker_states", "Current state of circuit breakers"),
            &["state"],
        ).unwrap();
        circuit_breaker_states.with_label_values(&["init"]).set(0); // Initialize with a label value
        registry.register(Box::new(circuit_breaker_states.clone())).unwrap();

        let rate_limiter_hits_opts = Opts::new("rate_limiter_hits", "Number of rate limiter hits");
        let rate_limiter_hits = IntCounter::with_opts(rate_limiter_hits_opts).unwrap();
        registry.register(Box::new(rate_limiter_hits.clone())).unwrap();

        let cache_hits_opts = Opts::new("cache_hits", "Number of cache hits");
        let cache_hits = IntCounter::with_opts(cache_hits_opts).unwrap();
        registry.register(Box::new(cache_hits.clone())).unwrap();

        let cache_misses_opts = Opts::new("cache_misses", "Number of cache misses");
        let cache_misses = IntCounter::with_opts(cache_misses_opts).unwrap();
        registry.register(Box::new(cache_misses.clone())).unwrap();

        let transform_counts = IntCounterVec::new(
            Opts::new("transform_counts", "Number of request and response transformations"),
            &["type"],
        ).unwrap();
        transform_counts.with_label_values(&["init"]).inc(); // Initialize with a label value
        registry.register(Box::new(transform_counts.clone())).unwrap();

        let authentication_attempts = IntCounterVec::new(
            Opts::new("authentication_attempts", "Number of authentication attempts"),
            &["type", "status"],
        ).unwrap();
        authentication_attempts.with_label_values(&["init", "init"]).inc(); // Initialize with label values
        registry.register(Box::new(authentication_attempts.clone())).unwrap();

        let timeout_counts_opts = Opts::new("timeout_counts", "Total number of request timeouts");
        let timeout_counts = IntCounter::with_opts(timeout_counts_opts).unwrap();
        registry.register(Box::new(timeout_counts.clone())).unwrap();

        let connection_errors_opts = Opts::new("connection_errors", "Total number of connection errors");
        let connection_errors = IntCounter::with_opts(connection_errors_opts).unwrap();
        registry.register(Box::new(connection_errors.clone())).unwrap();

        Metrics {
            registry,
            http_requests_total,
            ongoing_requests,
            http_responses_total,
            successful_requests,
            failed_requests,
            request_duration_seconds,
            request_size_bytes,
            response_size_bytes,
            error_counts,
            http_method_counts,
            status_code_counts,
            retry_counts,
            circuit_breaker_states,
            rate_limiter_hits,
            cache_hits,
            cache_misses,
            transform_counts,
            authentication_attempts,
            timeout_counts,
            connection_errors,
        }
    }

    pub fn gather_metrics(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        let metrics = Metrics::new();

        // Gather the metrics and print the names for inspection
        let metric_families = metrics.registry.gather();
        let mut metric_names = Vec::new();
        for family in metric_families {
            metric_names.push(family.get_name().to_string());
        }

        println!("{:?}", metric_names);

        // Verify that each metric is successfully registered by checking its presence in the registry's output
        assert!(metric_names.contains(&"http_requests_total".to_string()));
        assert!(metric_names.contains(&"ongoing_requests".to_string()));
        assert!(metric_names.contains(&"http_responses_total".to_string()));
        assert!(metric_names.contains(&"successful_requests".to_string()));
        assert!(metric_names.contains(&"failed_requests".to_string()));
        assert!(metric_names.contains(&"request_duration_seconds".to_string()));
        assert!(metric_names.contains(&"request_size_bytes".to_string()));
        assert!(metric_names.contains(&"response_size_bytes".to_string()));
        assert!(metric_names.contains(&"error_counts".to_string()));
        assert!(metric_names.contains(&"http_method_counts".to_string()));
        assert!(metric_names.contains(&"status_code_counts".to_string()));
        assert!(metric_names.contains(&"retry_counts".to_string()));
        assert!(metric_names.contains(&"circuit_breaker_states".to_string()));
        assert!(metric_names.contains(&"rate_limiter_hits".to_string()));
        assert!(metric_names.contains(&"cache_hits".to_string()));
        assert!(metric_names.contains(&"cache_misses".to_string()));
        assert!(metric_names.contains(&"transform_counts".to_string()));
        assert!(metric_names.contains(&"authentication_attempts".to_string()));
        assert!(metric_names.contains(&"timeout_counts".to_string()));
        assert!(metric_names.contains(&"connection_errors".to_string()));
    }

    #[test]
    fn test_gather_metrics() {
        let metrics = Metrics::new();

        // Increment some metrics to simulate activity
        metrics.http_requests_total.inc();
        metrics.successful_requests.inc();
        metrics.failed_requests.inc();
        metrics.request_duration_seconds.observe(0.5);
        metrics.request_size_bytes.observe(1024.0);
        metrics.response_size_bytes.observe(2048.0);
        metrics.error_counts.with_label_values(&["timeout"]).inc();
        metrics.http_method_counts.with_label_values(&["GET"]).inc();
        metrics.status_code_counts.with_label_values(&["200"]).inc();
        metrics.retry_counts.inc();
        metrics.circuit_breaker_states.with_label_values(&["open"]).set(1);
        metrics.rate_limiter_hits.inc();
        metrics.cache_hits.inc();
        metrics.cache_misses.inc();
        metrics.transform_counts.with_label_values(&["request"]).inc();
        metrics.authentication_attempts.with_label_values(&["JWT", "success"]).inc();
        metrics.timeout_counts.inc();
        metrics.connection_errors.inc();

        let metrics_output = metrics.gather_metrics();

        assert!(metrics_output.contains("http_requests_total"));
        assert!(metrics_output.contains("successful_requests"));
        assert!(metrics_output.contains("failed_requests"));
        assert!(metrics_output.contains("request_duration_seconds"));
        assert!(metrics_output.contains("request_size_bytes"));
        assert!(metrics_output.contains("response_size_bytes"));
        assert!(metrics_output.contains("error_counts"));
        assert!(metrics_output.contains("http_method_counts"));
        assert!(metrics_output.contains("status_code_counts"));
        assert!(metrics_output.contains("retry_counts"));
        assert!(metrics_output.contains("circuit_breaker_states"));
        assert!(metrics_output.contains("rate_limiter_hits"));
        assert!(metrics_output.contains("cache_hits"));
        assert!(metrics_output.contains("cache_misses"));
        assert!(metrics_output.contains("transform_counts"));
        assert!(metrics_output.contains("authentication_attempts"));
        assert!(metrics_output.contains("timeout_counts"));
        assert!(metrics_output.contains("connection_errors"));
    }
}
