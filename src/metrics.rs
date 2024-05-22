use prometheus::{Encoder, Histogram, IntCounter, IntGauge, Opts, Registry, TextEncoder, HistogramOpts, IntCounterVec};

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
        registry.register(Box::new(error_counts.clone())).unwrap();

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
