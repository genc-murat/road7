# Road7 - Advanced HTTP Proxy Server

<div align="center">
    <img src="/road7logo.png">
</div>


Road7 is a sophisticated HTTP proxy server implemented in Rust, designed to handle complex web traffic scenarios with features such as retries, circuit breaking, rate limiting, caching, and request/response transformations. Built with `hyper`, `tokio`, and `serde` libraries, Road7 ensures high performance and flexibility.

## Features

- **Retries**: Configurable retry strategies to handle transient failures.
- **Circuit Breaking**: Protects services from being overwhelmed by failures.
- **Rate Limiting**: Limits the rate of requests to ensure fair usage and prevent abuse.
- **Caching**: Caches responses to improve performance and reduce load on backend services.
- **Request/Response Transformations**: Modify requests and responses as they pass through the proxy.
- **Logging**: Configurable logging levels for detailed request and response logging.

## Configuration

Road7 is configured using a `config.toml` file, which defines the server settings, target endpoints, and their specific configurations.

### Configuration Fields

#### `ProxyConfig`
- **server**: The server configuration (`ServerConfig`).
- **targets**: A list of target configurations (`Target`).
- **retries**: Global retry configuration (`RetryConfig`).
- **default_circuit_breaker_config**: Default circuit breaker configuration (`CircuitBreakerConfig`).
- **default_timeout_seconds**: Default timeout in seconds for all targets (default: `30`).
- **default_rate_limiter_config**: Optional default rate limiter configuration (`RateLimiterConfig`).

#### `ServerConfig`
- **host**: The host address to bind the server to.
- **port**: The port number to listen on.
- **max_logging_level**: Maximum logging level (`DEBUG`, `INFO`, `WARN`, `ERROR`).
- **pool_size**: Connection pool size (default: `10`).
- **recv_buffer_size**: Optional receive buffer size.
- **send_buffer_size**: Optional send buffer size.

#### `Target`
- **path**: The path pattern to match for routing requests.
- **url**: The target URL to which the requests should be proxied.
- **retries**: Optional retry configuration for this target (`RetryConfig`).
- **request_transforms**: Optional list of request transformations (`Transform`).
- **response_transforms**: Optional list of response transformations (`Transform`).
- **circuit_breaker_config**: Optional circuit breaker configuration for this target (`CircuitBreakerConfig`).
- **rate_limiter_config**: Optional rate limiter configuration for this target (`RateLimiterConfig`).
- **routing_header**: Optional header for routing decisions.
- **routing_values**: Optional map of header values to target URLs.
- **timeout_seconds**: Optional timeout in seconds for this target.
- **cache_config**: Optional caching configuration for this target (`CacheConfig`).
- **logging_config**: Optional logging configuration for this target (`LoggingConfig`).

#### `LoggingConfig`
- **log_requests**: Whether to log incoming requests.
- **log_responses**: Whether to log outgoing responses.

#### `Transform`
- **transform_type**: The type of transformation (`header`).
- **name**: The name of the header to be transformed.
- **value**: The value to be set for the header (optional).
- **operation**: The operation to be performed (`Set`, `Remove`, `Append`).

#### `CircuitBreakerConfig`
- **max_failures**: Maximum number of failures before opening the circuit.
- **reset_timeout_seconds**: Time in seconds to wait before transitioning from open to half-open state.
- **half_open_attempts**: Number of attempts in half-open state before transitioning to closed or open state.

#### `RateLimiterConfig`
- **TokenBucket**: Token bucket rate limiter configuration.
  - **refill_rate**: Number of tokens refilled per second.
  - **burst_capacity**: Maximum number of tokens.
  - **header_key**: Optional header key for dynamic rate limiting.
- **LeakyBucket**: Leaky bucket rate limiter configuration.
  - **leak_rate**: Number of tokens leaked per second.
  - **bucket_size**: Maximum number of tokens.
  - **header_key**: Optional header key for dynamic rate limiting.
- **FixedWindow**: Fixed window rate limiter configuration.
  - **rate**: Maximum number of requests per window.
  - **window_seconds**: Duration of the window in seconds.
  - **header_key**: Optional header key for dynamic rate limiting.
- **SlidingLog**: Sliding log rate limiter configuration.
  - **rate**: Maximum number of requests per window.
  - **window_seconds**: Duration of the window in seconds.
  - **header_key**: Optional header key for dynamic rate limiting.
- **SlidingWindow**: Sliding window rate limiter configuration.
  - **rate**: Maximum number of requests per window.
  - **window_seconds**: Duration of the window in seconds.
  - **header_key**: Optional header key for dynamic rate limiting.

#### `RetryConfig`
- **strategy**: Retry strategy (`ExponentialBackoff`, `LinearBackoff`, `FixedInterval`, `RandomDelay`, `IncrementalBackoff`, `FibonacciBackoff`, `GeometricBackoff`, `HarmonicBackoff`, `JitterBackoff`).
- **base_delay_seconds**: Base delay between retries in seconds.
- **max_attempts**: Maximum number of retry attempts.
- **factor**: Optional factor for exponential and geometric backoff strategies.
- **step_delay_seconds**: Optional step delay for linear and incremental backoff strategies.

#### `CacheConfig`
- **ttl_seconds**: Time to live in seconds for cache entries.
- **max_size**: Maximum number of cache entries.
- **serialize**: Whether to serialize data in the cache.

### Example Configuration (`config.toml`)

```toml
[server]
host = "127.0.0.1"
port = 8080
max_logging_level = "INFO"
pool_size = 10
recv_buffer_size = 8192
send_buffer_size = 8192

[retries]
strategy = "ExponentialBackoff"
base_delay_seconds = 1
max_attempts = 3

[default_circuit_breaker_config]
max_failures = 3
reset_timeout_seconds = 30
half_open_attempts = 1

default_timeout_seconds = 30

[[targets]]
path = "/api/service1"
url = "http://localhost:8081"
retries = { strategy = "FixedInterval", base_delay_seconds = 2, max_attempts = 5 }
request_transforms = [
    { type = "header", name = "X-Request-ID", value = "12345", operation = "Set" }
]
response_transforms = [
    { type = "header", name = "X-Response-Time", operation = "Remove" }
]
circuit_breaker_config = { max_failures = 5, reset_timeout_seconds = 60, half_open_attempts = 3 }
rate_limiter_config = { type = "TokenBucket", refill_rate = 10, burst_capacity = 20 }
timeout_seconds = 10
cache_config = { ttl_seconds = 60, max_size = 100, serialize = true }
logging_config = { log_requests = true, log_responses = true }
```

## Running the Proxy Server

### Prerequisites

- Rust and Cargo installed on your system.

### Steps

1. Clone the repository:
    ```sh
    git clone https://github.com/genc-murat/road7.git
    cd road7
    ```

2. Create a `config.toml` file based on the example configuration provided above.

3. Build and run the proxy server:
    ```sh
    cargo build --release
    ./target/release/road7
    ```

## Detailed Explanation of Components

### `ProxyConfig`

Holds the main configuration for the proxy, including server settings, retry strategies, circuit breaker configurations, and rate limiter settings.

### `ServerConfig`

Defines the server settings, such as host, port, logging level, and connection pool size.

### `Target`

Represents a backend service to which requests are proxied. Each target has its own configurations for retries, circuit breaking, rate limiting, caching, and logging.

### `CircuitBreaker`

Manages the state of the circuit breaker for a target, ensuring that requests are not sent to unhealthy services.

### `RateLimiter`

Implements various rate limiting strategies to control the flow of requests.

### `Cache`

Provides caching functionality to store responses and reduce the load on backend services.

### Request and Response Transformations

Allows modification of requests and responses using a series of transformations defined in the configuration.

## Logging

The proxy server uses `tracing` for logging, with configurable logging levels. Logs can be directed to various outputs and provide detailed information about incoming requests, outgoing responses, and internal operations.

## Contributing

Contributions are welcome! Please fork the repository and submit a pull request with your changes. Ensure that your code adheres to the existing style and includes appropriate tests.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Contact

For any questions or feedback, please open an issue on GitHub or contact the repository maintainer.