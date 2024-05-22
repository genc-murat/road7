
    use prometheus::{Encoder, IntCounter, IntGauge, Opts, Registry, TextEncoder};

    #[derive(Clone)]
    pub struct Metrics {
        pub registry: Registry,
        pub http_requests_total: IntCounter,
        pub ongoing_requests: IntGauge,
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

            Metrics {
                registry,
                http_requests_total,
                ongoing_requests,
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

