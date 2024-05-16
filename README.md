# Road7 Proxy Server

<div align="center">
    <img src="/road7logo.png">
</div>


## Overview

Road7 is a high-performance, feature-rich proxy server implemented in Rust. It leverages `hyper`, `tokio`, and `serde` to provide sophisticated functionalities such as retries, circuit breaking, rate limiting, caching, request/response transformations, and security headers. This server is highly configurable, allowing fine-tuned control over its behavior and performance.

## Features

- **Retries**: Configurable retry strategies to handle transient failures.
- **Circuit Breaking**: Automatically opens and closes circuits based on failure rates.
- **Rate Limiting**: Multiple rate limiting strategies to control request flow.
- **Caching**: Configurable response caching for improved performance.
- **Request/Response Transformations**: Modify headers and content dynamically.
- **Security Headers**: Enhanced security with configurable HTTP headers.
- **Logging**: Detailed request and response logging.
- **CORS Support**: Configurable Cross-Origin Resource Sharing.

## Configuration

Road7 uses a configuration file (in `toml` format) to manage its settings. Below is a detailed description of each configuration section and its options.

### Example Configuration File (config.toml)

```toml
[server]
host = "127.0.0.1"
port = 8080
max_logging_level = "INFO"
pool_size = 10
recv_buffer_size = 8192
send_buffer_size = 8192

[runtime]
worker_threads = 4

[[targets]]
path = "/api"
url = "https://backend.service/api"
timeout_seconds = 30

[targets.retries]
strategy = "ExponentialBackoff"
base_delay_seconds = 2
max_attempts = 3
factor = 2.0

[[targets.request_transforms]]
type = "Header"
name = "X-Forwarded-For"
value = "127.0.0.1"
operation = "Set"

[[targets.response_transforms]]
type = "Header"
name = "Cache-Control"
value = "no-store"
operation = "Set"

[targets.circuit_breaker_config]
max_failures = 5
reset_timeout_seconds = 60
half_open_attempts = 2

[targets.rate_limiter_config]
type = "TokenBucket"
refill_rate = 10
burst_capacity = 50

[targets.cache_config]
ttl_seconds = 300
max_size = 1024
serialize = true

[targets.logging_config]
log_requests = true
log_responses = true

[targets.cors_config]
enabled = true
allow_origin = "*"
allow_headers = "*"
allow_methods = "GET,POST,PUT,DELETE,OPTIONS"

[security_headers_config]
strict_transport_security = "max-age=31536000; includeSubDomains"
x_content_type_options = "nosniff"
x_frame_options = "DENY"
content_security_policy = "default-src 'self'; script-src 'self'; object-src 'none'; style-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"
x_xss_protection = "1; mode=block"
referrer_policy = "no-referrer"
permissions_policy = "geolocation=(), microphone=(), camera=()"
```

### Configuration Options

#### Server Configuration

- `host`: The host address to bind the proxy server.
- `port`: The port number to bind the proxy server.
- `max_logging_level`: The maximum logging level (e.g., DEBUG, INFO, WARN, ERROR).
- `pool_size`: The maximum number of idle connections per host.
- `recv_buffer_size`: The size of the receive buffer.
- `send_buffer_size`: The size of the send buffer.

#### Runtime Configuration

- `worker_threads`: The number of worker threads to use.

#### Target Configuration

Each target configuration specifies the details for routing and handling requests to a specific backend service.

- `path`: The path prefix to match incoming requests.
- `url`: The URL of the backend service.
- `timeout_seconds`: The timeout duration for requests in seconds.

#### Retry Configuration

Specifies the retry strategy for handling transient failures.

- `strategy`: The retry strategy (e.g., ExponentialBackoff, LinearBackoff, FixedInterval).
- `base_delay_seconds`: The base delay between retries in seconds.
- `max_attempts`: The maximum number of retry attempts.
- `factor`: The factor by which to multiply the delay for exponential backoff.

#### Transformations

Configures request and response transformations.

- `type`: The type of transformation (e.g., Header).
- `name`: The name of the header to transform.
- `value`: The value to set for the header.
- `operation`: The operation to perform (e.g., Set, Remove, Append).

#### Circuit Breaker Configuration

Specifies the circuit breaker settings.

- `max_failures`: The maximum number of failures before opening the circuit.
- `reset_timeout_seconds`: The duration to wait before transitioning to half-open state.
- `half_open_attempts`: The number of attempts allowed in the half-open state.

#### Rate Limiter Configuration

Configures rate limiting to control request flow.

- `type`: The type of rate limiter (e.g., TokenBucket, LeakyBucket, FixedWindow, SlidingLog, SlidingWindow).
- `refill_rate`: The rate at which tokens are refilled (for TokenBucket).
- `burst_capacity`: The maximum burst capacity (for TokenBucket).

#### Cache Configuration

Specifies the caching settings.

- `ttl_seconds`: The time-to-live for cached responses in seconds.
- `max_size`: The maximum size of the cache.
- `serialize`: Whether to serialize cached responses.

#### Logging Configuration

Configures request and response logging.

- `log_requests`: Whether to log incoming requests.
- `log_responses`: Whether to log outgoing responses.

#### CORS Configuration

Specifies the Cross-Origin Resource Sharing settings.

- `enabled`: Whether CORS is enabled.
- `allow_origin`: The allowed origins for CORS.
- `allow_headers`: The allowed headers for CORS.
- `allow_methods`: The allowed methods for CORS.

#### Security Headers Configuration

Configures security-related HTTP headers.

- `strict_transport_security`: The value for the `Strict-Transport-Security` header.
- `x_content_type_options`: The value for the `X-Content-Type-Options` header.
- `x_frame_options`: The value for the `X-Frame-Options` header.
- `content_security_policy`: The value for the `Content-Security-Policy` header.
- `x_xss_protection`: The value for the `X-XSS-Protection` header.
- `referrer_policy`: The value for the `Referrer-Policy` header.
- `permissions_policy`: The value for the `Permissions-Policy` header.

## Detailed Example

### Server Configuration

The server section defines the general settings for the proxy server, including the host, port, logging level, connection pool size, and buffer sizes.

```toml
[server]
host = "127.0.0.1"  # The host address to bind the proxy server
port = 8080  # The port number to bind the proxy server
max_logging_level = "INFO"  # The maximum logging level (e.g., DEBUG, INFO, WARN, ERROR)
pool_size = 10  # The maximum number of idle connections per host
recv_buffer_size = 8192  # The size of the receive buffer
send_buffer_size = 8192  # The size of the send buffer
```

### Runtime Configuration

The runtime section defines the number of worker threads to be used by the proxy server.

```toml
[runtime]
worker_threads = 4  # The number of worker threads to use
```

### Target Configuration

Each target section defines the routing and handling details for a specific backend service.

```toml
[[targets]]
path = "/api"  # The path prefix to match incoming requests
url = "https://backend.service/api"  # The URL of the backend service
timeout_seconds = 30  # The timeout duration for requests in seconds

[targets.retries]
strategy = "ExponentialBackoff"  # The retry strategy (e.g., ExponentialBackoff, LinearBackoff, FixedInterval)
base_delay_seconds = 2  # The base delay between retries in seconds
max_attempts = 3  # The maximum number of retry attempts
factor = 2.0  # The factor by which to multiply the delay for exponential backoff

[[targets.request_transforms]]
type = "Header"  # The type of transformation (e.g., Header)
name = "X-Forwarded-For"  # The name of the header to transform
value = "127.0.0.1"  # The value to set for the header
operation = "Set"  # The operation to perform (e.g., Set, Remove, Append)

[[targets.response_transforms]]
type = "Header"
name = "Cache-Control"
value = "no-store"
operation = "Set"

[targets.circuit_breaker_config]
max_failures = 5  # The maximum number of failures before opening the circuit
reset_timeout_seconds = 60  # The duration to wait before transitioning to half-open state
half_open_attempts = 2  # The number of attempts allowed in the half-open state

[targets.rate_limiter_config]
type = "TokenBucket"  # The type of rate limiter (e.g., TokenBucket, LeakyBucket, FixedWindow, SlidingLog, SlidingWindow)
refill_rate = 10  # The rate at which tokens are refilled (for TokenBucket)
burst_capacity = 50  # The maximum burst capacity (for TokenBucket)

```toml
[targets.cache_config]
ttl_seconds = 300  # The time-to-live for cached responses in seconds
max_size = 1024  # The maximum size of the cache
serialize = true  # Whether to serialize cached responses

[targets.logging_config]
log_requests = true  # Whether to log incoming requests
log_responses = true  # Whether to log outgoing responses

[targets.cors_config]
enabled = true  # Whether CORS is enabled
allow_origin = "*"  # The allowed origins for CORS
allow_headers = "*"  # The allowed headers for CORS
allow_methods = "GET,POST,PUT,DELETE,OPTIONS"  # The allowed methods for CORS
```

### Security Headers Configuration

The security headers section configures various HTTP headers to enhance security.

```toml
[security_headers_config]
strict_transport_security = "max-age=31536000; includeSubDomains"  # The value for the `Strict-Transport-Security` header
x_content_type_options = "nosniff"  # The value for the `X-Content-Type-Options` header
x_frame_options = "DENY"  # The value for the `X-Frame-Options` header
content_security_policy = "default-src 'self'; script-src 'self'; object-src 'none'; style-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"  # The value for the `Content-Security-Policy` header
x_xss_protection = "1; mode=block"  # The value for the `X-XSS-Protection` header
referrer_policy = "no-referrer"  # The value for the `Referrer-Policy` header
permissions_policy = "geolocation=(), microphone=(), camera=()"  # The value for the `Permissions-Policy` header
```

### Detailed Explanation of Configuration Options

#### Server Configuration

- **host**: The host address to bind the proxy server. It can be an IP address or a domain name.
- **port**: The port number to bind the proxy server. Ensure that the port is not already in use.
- **max_logging_level**: Specifies the maximum level of logging. Valid values are `DEBUG`, `INFO`, `WARN`, `ERROR`.
- **pool_size**: Sets the maximum number of idle connections per host. This can improve performance by reusing existing connections.
- **recv_buffer_size**: Configures the size of the receive buffer in bytes.
- **send_buffer_size**: Configures the size of the send buffer in bytes.

#### Runtime Configuration

- **worker_threads**: Specifies the number of worker threads for the server. More threads can handle more simultaneous requests but consume more system resources.

#### Target Configuration

Each target defines how incoming requests matching the specified path should be handled and forwarded to the backend service.

- **path**: The path prefix to match incoming requests. All requests starting with this path will be routed to the specified target.
- **url**: The URL of the backend service to forward the requests to.
- **timeout_seconds**: Specifies the maximum duration in seconds to wait for a response from the backend service.

#### Retry Configuration

Configures how the proxy should retry failed requests to the backend service.

- **strategy**: Defines the retry strategy to use. Options include `ExponentialBackoff`, `LinearBackoff`, `FixedInterval`, etc.
- **base_delay_seconds**: The initial delay between retries in seconds.
- **max_attempts**: The maximum number of retry attempts.
- **factor**: The factor by which to multiply the delay for exponential backoff strategies.

#### Transformations

You can define transformations to apply to requests and responses, such as modifying headers.

- **type**: The type of transformation, currently supporting `Header`.
- **name**: The name of the header to transform.
- **value**: The value to set for the header.
- **operation**: The operation to perform, such as `Set`, `Remove`, or `Append`.

#### Circuit Breaker Configuration

Circuit breakers help in preventing system overload by controlling traffic to a failing service.

- **max_failures**: The number of failures before the circuit breaker opens.
- **reset_timeout_seconds**: The time to wait before attempting to close the circuit breaker.
- **half_open_attempts**: The number of allowed attempts in the half-open state before deciding to close or re-open the circuit.

#### Rate Limiter Configuration

Rate limiting helps in controlling the number of requests to prevent abuse and maintain performance.

- **type**: The type of rate limiter, such as `TokenBucket`, `LeakyBucket`, `FixedWindow`, `SlidingLog`, or `SlidingWindow`.
- **refill_rate**: For `TokenBucket`, the rate at which tokens are refilled.
- **burst_capacity**: For `TokenBucket`, the maximum number of tokens that can be accumulated.

#### Cache Configuration

Caches responses to improve performance and reduce load on backend services.

- **ttl_seconds**: The time-to-live for cached responses in seconds.
- **max_size**: The maximum number of responses to keep in the cache.
- **serialize**: Whether to serialize cached responses for persistence.

#### Logging Configuration

Configures logging for requests and responses.

- **log_requests**: If `true`, logs incoming requests.
- **log_responses**: If `true`, logs outgoing responses.

#### CORS Configuration

Configures Cross-Origin Resource Sharing settings to control how resources are shared across different origins.

- **enabled**: Enables or disables CORS.
- **allow_origin**: Specifies the allowed origins for CORS.
- **allow_headers**: Specifies the allowed headers for CORS.
- **allow_methods**: Specifies the allowed methods for CORS.

#### Security Headers Configuration

Enhances security by configuring various HTTP headers.

- **strict_transport_security**: Sets the `Strict-Transport-Security` header value.
- **x_content_type_options**: Sets the `X-Content-Type-Options` header value.
- **x_frame_options**: Sets the `X-Frame-Options` header value.
- **content_security_policy**: Sets the `Content-Security-Policy` header value.
- **x_xss_protection**: Sets the `X-XSS-Protection` header value.
- **referrer_policy**: Sets the `Referrer-Policy` header value.
- **permissions_policy**: Sets the `Permissions-Policy` header value.

## Running the Proxy

To run the proxy server, ensure that you have the configuration file (`config.toml`) in place, and then start the server using the command:

```sh
cargo run --release
```

The server will start and listen on the configured host and port, handling requests according to the specified configuration.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License.

