

# Road7 Proxy Server

<div align="center">
    <img src="/road7logo.png">
</div>

`road7` is a sophisticated and highly customizable proxy server built with Rust. It is designed to handle a variety of proxy functionalities, making it suitable for enterprise-level applications. With a focus on flexibility and performance, `road7` offers a wide range of features, including authentication, rate limiting, caching, retries, circuit breaking, security headers, bot detection, and detailed metrics collection. This README provides comprehensive information on the features and configurations of `road7`, enabling users to effectively deploy and manage the proxy server in various environments.

## Overview

`road7` is built to address the complex needs of modern web applications. It serves as a reverse proxy, routing incoming client requests to appropriate backend services based on defined rules and configurations. The proxy server is designed with performance and scalability in mind, making it an ideal choice for large-scale deployments. Its modular architecture allows for easy extension and customization, ensuring it can adapt to specific requirements and use cases.

## Features

### Authentication
- **Basic Authentication**: Validates requests using a username and password.
  - Requires the `Authorization` header in the request.
  - Credentials are encoded in Base64 and checked against a predefined list of valid users.
- **JWT Authentication**: Validates requests using JSON Web Tokens (JWT).
  - Requires the `Authorization` header with a Bearer token.
  - The token is decoded and verified using a secret key.

### Rate Limiting
- **Token Bucket Algorithm**: Configurable rate limiting per target or endpoint using the token bucket algorithm.
  - Controls the rate of requests to prevent overloading the server.
  - Tokens are added to the bucket at a fixed rate, and requests consume tokens.
- **Burst Capacity**: Supports burst capacity and customizable period for rate limiting.
  - Allows a temporary burst of traffic by permitting more requests than the regular rate.
- **Rate Limiter Manager**: Manages multiple rate limiters and provides status information.
  - Can add and manage rate limiters dynamically based on the configuration.

### Caching
- **Read-Through Cache**: Caches responses based on configurable TTL (Time-To-Live).
  - Reduces backend load by serving cached responses for repeated requests.
- **ETag and Last-Modified Headers**: Utilizes ETag and Last-Modified headers for cache validation.
  - Ensures clients receive updated content only when it has changed.
- **Cache Cleanup**: Periodically cleans up expired cache entries.
  - Maintains cache efficiency and frees up memory by removing outdated entries.

### Retries
- **Retry Strategies**: Supports various retry strategies including Fixed Interval, Exponential Backoff, Linear Backoff, and more.
  - Ensures request reliability by retrying failed requests based on the configured strategy.
- **Retry Configuration**: Allows customization of retry strategy, base delay, maximum attempts, and optional jitter.
  - Configurable parameters to fine-tune retry behavior based on use case.

### Circuit Breaking
- **Circuit Breaker**: Manages the state of connections (Closed, Open, Half-Open) to prevent overwhelming the target servers.
  - Protects backend services by temporarily blocking requests when failures reach a threshold.
- **Configuration**: Customizable maximum failures, reset timeout, and half-open attempts.
  - Allows precise control over circuit breaker behavior to balance reliability and availability.

### Load Balancing
- **Algorithms**: Supports multiple load balancing algorithms such as Round Robin, Random, Least Connections, and IP Hash.
  - Distributes incoming requests across multiple backend servers to ensure balanced load.
- **Weighted Algorithms**: Supports weighted round robin and weighted least connections algorithms.
  - Allows assigning different weights to backend servers to reflect their capacity or priority.

### Bot Detection
- **User-Agent Based**: Detects bots based on User-Agent header patterns and rules.
  - Identifies and blocks requests from known bots to protect the service from abuse.
- **Configuration**: Allows specifying allowed, denied, and pattern-matched User-Agent strings.
  - Customizable rules to adapt to specific bot detection requirements.

### Security Headers
- **Predefined Headers**: Adds common security headers like Content Security Policy (CSP), Strict Transport Security (HSTS), X-Content-Type-Options, and more.
  - Enhances security by enforcing browser policies and protecting against common attacks.
- **Customization**: Configurable security header values.
  - Provides flexibility to set security headers based on specific requirements.

### Metrics
- **Prometheus Integration**: Collects and exposes metrics for Prometheus, including request rates, latencies, error rates, etc.
  - Enables monitoring and observability of the proxy server's performance and health.
- **Detailed Metrics**: Tracks various metrics like HTTP request counts, response counts, errors, and circuit breaker states.
  - Provides insights into server behavior and helps in troubleshooting issues.

### Transformations
- **Request and Response Transforms**: Allows transforming headers of requests and responses based on specified rules.
  - Enables modification of headers for specific use cases like adding custom headers or modifying existing ones.

### CORS (Cross-Origin Resource Sharing)
- **CORS Configuration**: Supports configuring CORS headers to control resource sharing policies.
  - Ensures proper handling of cross-origin requests by specifying allowed origins, headers, and methods.

### Logging
- **Configurable Logging**: Supports logging request and response details for debugging and monitoring.
  - Provides visibility into request handling and helps in diagnosing issues.

## Configuration

`road7` uses a TOML configuration file to define its settings. Below are the detailed configurations available:

### Server Configuration
Defines the basic server settings.

```toml
[server]
host = "0.0.0.0"  # The IP address the server will bind to.
port = 8080  # The port the server will listen on.
max_logging_level = "INFO"  # The maximum level of logging (e.g., DEBUG, INFO, WARN, ERROR).
pool_size = 10  # The number of idle connections to maintain in the pool.
recv_buffer_size = 8192  # The size of the receive buffer.
send_buffer_size = 8192  # The size of the send buffer.
```

### Runtime Configuration
Defines the runtime settings, particularly the number of worker threads.

```toml
[runtime]
worker_threads = 4  # The number of worker threads to handle requests.
```

### Target Configuration
Define the targets to which the proxy will route requests. Each target specifies the path and backend URLs, along with optional configurations for authentication, retries, circuit breaking, caching, CORS, rate limiting, and transformations.

```toml
[[targets]]
path = "/api"  # The path prefix for the target.
urls = ["http://backend1.example.com", "http://backend2.example.com"]  # List of backend URLs for load balancing.
authentication = { type = "Basic", basic_users = { "user1" = "password1" } }  # Authentication settings.
retries = { strategy = "ExponentialBackoff", base_delay_seconds = 2, max_attempts = 3, factor = 2.0 }  # Retry settings.
circuit_breaker_config = { max_failures = 5, reset_timeout_seconds = 60, half_open_attempts = 2 }  # Circuit breaker settings.
timeout_seconds = 30  # Timeout for requests to this target.
cache_config = { ttl_seconds = 60, max_size = 100, serialize = true }  # Cache settings.
cors_config = { enabled = true, "Access-Control-Allow-Origin" = "*", "Access-Control-Allow-Headers" = "Content-Type", "Access-Control-Allow-Methods" = "GET,POST,PUT,DELETE" }  # CORS settings.
rate_limiter_config = { capacity = 100, max_rate = 50, every = 60, burst_capacity = 10 }  # Rate limiter settings.
request_transforms = [{ type = "header", name = "X-Request-ID", value = "unique-id", operation = "Set" }]  # Request transformation settings.
response_transforms = [{ type = "header", name = "X-Response-Time", value = "ms", operation = "Append" }]  # Response transformation settings.
```

### Authentication Configuration
Supports Basic and JWT authentication.

```toml
[authentication]
type = "JWT"  # Type of authentication (JWT or Basic).
jwt_secret = "your_jwt_secret_key"  # Secret key for JWT validation.
```

### Retry Configuration
Supports various retry strategies.

```toml
[retries]
strategy = "FixedInterval"  # Retry strategy (FixedInterval, ExponentialBackoff, etc.).
base_delay_seconds = 2  # Base delay between retries in seconds.
max_attempts = 5  # Maximum number of retry attempts.
```

### Circuit Breaker Configuration
Defines the behavior for circuit breaking.

```toml
[circuit_breaker]
max_failures = 3  # Maximum number of failures before opening the circuit.
reset_timeout_seconds = 30  # Time to wait before transitioning from Open to Half-Open state.
half_open_attempts = 1  # Number of successful attempts in Half-Open state before closing the circuit.
```

### Rate Limiter Configuration
Defines the settings for rate limiting.

```toml
[rate_limiter]
capacity = 100  # Number of requests allowed in a time period.
max_rate = 10  # Maximum rate of requests per second.
every = 60  # Time period for rate limiting in seconds.
burst_capacity = 20  # Additional capacity for burst traffic.
```

### Security Headers Configuration
Defines the security headers to be added to responses.

```toml
[security_headers]
strict_transport_security = "max-age=31536000; includeSubDomains"  # HSTS header value.
x_content_type_options = "nosniff"  # X-Content-Type-Options header value.
x_frame_options = "DENY"  # X-Frame-Options header value.
content_security_policy = "default-src 'self'"  # Content Security Policy header value.
x_xss_protection = "1; mode=block"  # X-XSS-Protection header value.
referrer_policy = "no-referrer"  # Referrer-Policy header value.
permissions_policy = "geolocation=(self)"  # Permissions-Policy header value.
feature_policy = "geolocation 'self'"  # Feature-Policy header value.
```

### Load Balancer Configuration


Defines the load balancing algorithm and weights.

```toml
[load_balancer]
algorithm = "RoundRobin"  # Load balancing algorithm (RoundRobin, LeastConnections, etc.).
weights = { "/api" = [1, 2] }  # Weights for weighted load balancing algorithms.
```

### Bot Detector Configuration
Defines the rules for bot detection.

```toml
[bot_detector]
allow = ["Mozilla"]  # List of allowed User-Agent strings.
deny = ["BadBot"]  # List of denied User-Agent strings.
patterns = ["bot"]  # List of patterns to detect bots.
empty_user_agent_is_bot = true  # Treat requests with an empty User-Agent header as bots.
```

## Metrics Collection
Collects and exposes metrics for Prometheus.

```toml
[metrics]
enabled = true  # Enable Prometheus metrics collection.
```

### Example Configuration File
Below is an example of a complete configuration file:

```toml
[server]
host = "0.0.0.0"
port = 8080
max_logging_level = "INFO"
pool_size = 10
recv_buffer_size = 8192
send_buffer_size = 8192

[runtime]
worker_threads = 4

[[targets]]
path = "/api"
urls = ["http://backend1.example.com", "http://backend2.example.com"]
authentication = { type = "Basic", basic_users = { "user1" = "password1" } }
retries = { strategy = "ExponentialBackoff", base_delay_seconds = 2, max_attempts = 3, factor = 2.0 }
circuit_breaker_config = { max_failures = 5, reset_timeout_seconds = 60, half_open_attempts = 2 }
timeout_seconds = 30
cache_config = { ttl_seconds = 60, max_size = 100, serialize = true }
cors_config = { enabled = true, "Access-Control-Allow-Origin" = "*", "Access-Control-Allow-Headers" = "Content-Type", "Access-Control-Allow-Methods" = "GET,POST,PUT,DELETE" }
rate_limiter_config = { capacity = 100, max_rate = 50, every = 60, burst_capacity = 10 }
request_transforms = [{ type = "header", name = "X-Request-ID", value = "unique-id", operation = "Set" }]
response_transforms = [{ type = "header", name = "X-Response-Time", value = "ms", operation = "Append" }]

[metrics]
enabled = true

[security_headers]
strict_transport_security = "max-age=31536000; includeSubDomains"
x_content_type_options = "nosniff"
x_frame_options = "DENY"
content_security_policy = "default-src 'self'"
x_xss_protection = "1; mode=block"
referrer_policy = "no-referrer"
permissions_policy = "geolocation=(self)"
feature_policy = "geolocation 'self'"

[load_balancer]
algorithm = "RoundRobin"
weights = { "/api" = [1, 2] }

[bot_detector]
allow = ["Mozilla"]
deny = ["BadBot"]
patterns = ["bot"]
empty_user_agent_is_bot = true
```

## Running road7
To run `road7`, use the following command:

```sh
cargo run --release
```

Ensure the configuration file `config.toml` is present in the root directory.

## Contributing
Contributions are welcome! Please submit a pull request or open an issue to discuss any changes.

## License
`road7` is licensed under the MIT License. See the LICENSE file for more information.
