# Road7

<div align="center">
    <img src="/road7logo.png">
</div>

Road7 is a highly configurable and efficient HTTP proxy server designed to handle a multitude of scenarios including dynamic routing, retries, circuit breaking, rate limiting, transformations, and caching. It is built using Rust and the Hyper library to ensure high performance and concurrency.

## Features Overview

- **Dynamic Request Routing**: The proxy can dynamically route requests based on the request path and optional headers. This allows the proxy to direct traffic to different backend services based on the path and headers of incoming requests.

- **Retries**: The proxy supports various retry strategies such as exponential backoff, linear backoff, fixed interval, and more. This helps to ensure that temporary failures in backend services can be smoothed out by retrying failed requests according to the specified strategy.

- **Circuit Breaking**: To prevent cascading failures in dependent services, the proxy implements a circuit breaker pattern. This temporarily blocks requests to services that are deemed unhealthy, giving them time to recover before accepting more traffic.

- **Rate Limiting**: Road7 supports multiple rate limiting strategies including Token Bucket, Leaky Bucket, Fixed Window, Sliding Log, and Sliding Window. This helps to control the rate at which requests are sent to backend services, preventing overload.

- **Transformations**: The proxy allows for request and response transformations where headers can be modified on the fly based on the configuration. This is useful for tasks like adding authentication tokens, setting CORS headers, and more.

- **Caching**: Responses can be cached based on a configurable TTL (time-to-live) and size. This reduces the load on backend services and improves response times for cached requests.

## Installation

### Prerequisites

- **Rust and Cargo**: Ensure you have the latest stable version of Rust and Cargo installed on your system.
- **Tokio runtime**: Road7 uses the Tokio runtime for asynchronous operations.

### Getting Started

1. **Clone the repository**:
   Clone the Road7 repository using Git and move into the project directory:
   ```bash
   git clone https://github.com/genc-murat/road7.git
   cd road7
   ```

2. **Build the project**:
   Compile the project using Cargo:
   ```bash
   cargo build --release
   ```

3. **Run the proxy server**:
   Start the proxy server with:
   ```bash
   cargo run --release
   ```

## Configuration

The proxy server's behavior is defined in a `config.toml` file located at the root of the project. This file contains several sections that allow you to specify how the proxy operates.

```toml
[server]
host = "127.0.0.1"
port = 8080

[retries]
strategy = "ExponentialBackoff"
base_delay_seconds = 1
max_attempts = 3
factor = 2.0

[default_circuit_breaker_config]
max_failures = 5
reset_timeout_seconds = 30
half_open_attempts = 2

default_timeout_seconds = 30

[[targets]]
path = "/api/service1"
url = "http://localhost:4000/api"
retries = { strategy = "FixedInterval", base_delay_seconds = 1, max_attempts = 5 }
request_transforms = [{ type = "header", name = "X-Service", value = "service1", operation = "Set" }]
response_transforms = [{ type = "header", name = "X-Proxy", value = "road7", operation = "Set" }]
circuit_breaker_config = { max_failures = 3, reset_timeout_seconds = 60, half_open_attempts = 1 }
rate_limiter_config = { type = "TokenBucket", refill_rate = 10, burst_capacity = 20 }
timeout_seconds = 20
cache_config = { ttl_seconds = 120, max_size = 1000, serialize = true }
```

### Detailed Configuration

#### ProxyConfig

- **server**: Defines the host and port where the proxy will listen for incoming HTTP requests.
  
  - `host`: The hostname or IP address of the server.
  - `port`: The port number on which the server accepts connections.

- **retries**: Global default retry configuration that applies if no specific retry settings are defined for a target.

  - `strategy`: Type of retry strategy (e.g., `ExponentialBackoff`).
  - `base_delay_seconds`: Base delay in seconds between retries.
  - `max_attempts`: Maximum number of retry attempts.
  - `factor`: The multiplier used for increasing the delay (for exponential backoff).

- **default_circuit_breaker_config**: Default circuit breaker configuration used if no specific settings are provided for a target.

  - `max_failures`: Number of failures before opening the circuit.
  - `reset_timeout_seconds`: Time in seconds to transition from open to half-open.
  - `half_open_attempts`: Allowed attempts in half-open state to test for recovery.

- **default_timeout_seconds**: Default timeout in seconds for forwarding requests if no specific timeout is defined for a target.

- **targets**: A list of targets with specific routing and operational rules.

  - `path`: Path prefix to match for routing the request.
  - `url`: URL of the backend service to which the request should be forwarded.
  - `retries`, `request_transforms`, `response_transforms`, `circuit_breaker_config`, `rate_limiter_config`, `timeout_seconds`, `cache_config`: Optional configurations that override the default settings for the specific target.

#### RetryConfig

Defines how retries should be handled for requests.

- **strategy**: The strategy for retrying (e.g., `ExponentialBackoff`, `LinearBackoff`, `FixedInterval`).
- **base_delay_seconds**: The initial delay between retries in seconds.
- **max_attempts**: Maximum number of retries allowed.
- **factor**: Growth factor for the delay (relevant for exponential strategies).
- **step_delay_seconds**: Fixed increment for the delay (relevant for linear strategies).

#### CircuitBreakerConfig

Configures the circuit breaker to avoid overwhelming failing services.

- **max_failures**: Number of failures that cause the circuit to open, stopping requests.
- **reset_timeout_seconds**: Duration in seconds to wait before attempting to close the circuit.
- **half_open_attempts**: Number of attempts allowed when the circuit is half-open before deciding to close or open it again.

#### RateLimiterConfig

Manages the rate limiting using different strategies.

- **TokenBucket**: Allows bursts of traffic, filling up at a continuous rate.
  - `refill_rate`: Tokens added per second.
  - `burst_capacity`: Maximum burst size in tokens.

- **LeakyBucket**: Ensures a steady output rate regardless of bursty inputs.
  - `leak_rate`: Tokens leaked per second, controlling the output rate.
  - `bucket_size`: Maximum size of the bucket.

- **FixedWindow**: Limits the number of requests within a fixed window of time.
  - `rate`: Maximum number of requests allowed.
  - `window_seconds`: Duration of the window in seconds.

- **SlidingLog**: Uses a log of timestamps to dynamically manage request limits over a sliding window.
- **SlidingWindow**: Similar to `FixedWindow` but adjusts the window continuously based on traffic.

#### CacheConfig

Settings for caching responses to reduce backend load.

- **ttl_seconds**: Time in seconds that a cache entry remains valid.
- **max_size**: Maximum number of entries the cache can hold.
- **serialize**: Indicates if the response should be serialized when cached.

## Usage

To use the proxy server, configure your `config.toml` as required and run the server. Here's how to interact with it:

### Making Requests

You can make HTTP requests to the proxy server using any HTTP client. For example, using `curl`:

```bash
curl http://localhost:8080/api/service1
```

This request is routed based on the path `/api/service1`. According to the `config.toml`, the proxy will route this to the configured backend service, applying retries, rate limiting, and other configurations as defined.

## Contributing

Your contributions are welcome to enhance the functionalities of Road7. Here’s how you can contribute:

1. **Fork the Repository**: Fork the project to your GitHub account.
2. **Create a Feature Branch**: Make a new branch for your feature (`git checkout -b my-new-feature`).
3. **Commit Changes**: Commit your changes (`git commit -am 'Add some feature'`).
4. **Push to the Branch**: Push your changes to your branch (`git push origin my-new-feature`).
5. **Submit a Pull Request**: Open a pull request to merge your changes into the main branch.

Feel free to submit issues and enhancement requests.