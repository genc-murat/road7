

# Road7 Proxy Server

<div align="center">
    <img src="/road7logo.png">
</div>

Road7 is a highly advanced, customizable proxy server built with Rust, designed to handle complex routing, security, and load balancing requirements. It stands out for its robust feature set and modular architecture, making it an ideal choice for developers and enterprises looking to implement a scalable and secure proxy solution. Road7 excels in providing flexibility, performance, and reliability, ensuring your applications can handle high traffic loads while maintaining security and efficiency.

## Features

Road7 boasts a comprehensive set of features that cater to various use cases, from simple request forwarding to sophisticated request transformations and security enhancements. Here are some of the standout features, explained in more detail:

### Load Balancing

Load balancing is essential for distributing incoming network traffic across multiple backend servers, ensuring no single server becomes overwhelmed. This enhances availability and reliability of your services.

- **RoundRobin**: Distributes incoming requests evenly across all backend servers, ensuring a balanced load. Ideal for scenarios where all backend servers have equal capacity and performance characteristics.
- **Random**: Selects a backend server at random, providing a simple yet effective load distribution. Useful in scenarios where simplicity is preferred over strict balancing.
- **LeastConnections**: Directs traffic to the server with the fewest active connections, optimizing resource usage. Perfect for environments with varying server loads, ensuring the least busy server gets new requests.
- **WeightedRoundRobin**: Allows you to assign different weights to servers, distributing traffic based on capacity or priority. Useful when backend servers have different processing capacities or priorities.
- **IPHash**: Uses the client’s IP address to consistently route requests to the same server, ideal for session persistence. This ensures that a client’s requests are handled by the same server, maintaining session state.
- **ConsistentHashing**: Ensures that similar requests are routed to the same backend server, reducing cache misses and improving performance. Suitable for caching scenarios where request patterns are predictable.
- **WeightedLeastConnections**: Combines weights and active connections to route requests more intelligently, balancing load and capacity. This hybrid approach optimizes for both server load and capacity.

### Caching

Caching reduces load on backend servers and improves response times by storing copies of responses and serving them directly for subsequent requests.

- **TTL-based Caching**: Configurable time-to-live (TTL) for cache entries to ensure fresh data. This ensures that cached data is not served indefinitely and gets refreshed periodically.
- **Max Size Limiting**: Limits the number of cache entries to optimize memory usage. Prevents the cache from growing too large and consuming excessive memory.
- **Serialization Options**: Optionally serialize cache entries to improve performance and storage efficiency. Serialized entries can be quickly stored and retrieved, enhancing cache performance.

### Authentication

Authentication ensures that only authorized users can access your services, protecting sensitive data and resources.

- **JWT Authentication**: Securely validate JSON Web Tokens, supporting various token issuers and secrets. This method provides a robust way to verify user identity and permissions.
- **Basic Authentication**: Simple, username-password based authentication with configurable user credentials. Useful for environments where simplicity is preferred and user credentials can be securely managed.

### Rate Limiting

Rate limiting controls the rate of incoming requests to prevent abuse and ensure fair usage of resources.

- **Token Bucket**: Smoothes out bursts of traffic by allowing requests to accumulate tokens over time. Ideal for scenarios with intermittent bursts of traffic.
- **Leaky Bucket**: Ensures a consistent rate of traffic, suitable for scenarios where a steady flow is required. This approach smooths out traffic over time.
- **Fixed Window**: Limits the number of requests per fixed time window, providing predictable rate limiting. Useful for enforcing strict rate limits over specific time periods.
- **Sliding Log**: Tracks request timestamps, allowing for more dynamic rate limiting based on recent activity. This method adapts to varying traffic patterns more dynamically.
- **Sliding Window**: Combines the benefits of fixed window and sliding log approaches, offering fine-grained control. Ensures that rate limits are enforced more smoothly.
- **Quota**: Sets a fixed quota of requests per time period, ideal for API usage limits. This is particularly useful for managing API usage and preventing overuse.
- **Dynamic**: Adjusts rate limits based on load, providing adaptive control over traffic flow. This approach is ideal for environments with varying traffic loads.

### Retries

Retries help to ensure reliability by re-attempting failed requests, using different strategies to minimize impact on the backend servers.

- **Fixed Interval**: Retries requests at a fixed interval, providing a straightforward retry mechanism. This is simple and predictable, ensuring consistent retry behavior.
- **Exponential Backoff**: Increases the delay between retries exponentially, reducing load during failures. This approach helps to prevent overwhelming backend services during outages.
- **Linear Backoff**: Gradually increases the delay between retries, balancing load and response times. This provides a middle ground between fixed and exponential backoff.
- **Random Delay**: Introduces randomness in retry delays, preventing thundering herd problems. This approach helps to distribute retry attempts more evenly.
- **Incremental Backoff**: Incrementally increases retry delays, combining predictability and flexibility. Ensures retries are spread out over time without overwhelming the backend.
- **Fibonacci Backoff**: Uses the Fibonacci sequence for retry delays, offering a unique and effective strategy. This approach combines incremental increase with randomness.
- **Geometric Backoff**: Multiplies delays by a constant factor, providing exponential-like behavior. Ensures retries are spread out more effectively over time.
- **Harmonic Backoff**: Reduces retry delays harmonically, offering a balance between retry attempts and delays. This approach provides a smooth increase in delay times.
- **Jitter Backoff**: Adds jitter to delays, combining other backoff strategies with randomness to prevent spikes. Ensures retry attempts are spread out and less predictable.

### Circuit Breaking

Circuit breaking prevents backend services from being overwhelmed by temporarily blocking requests when errors are detected.

- **Max Failures**: Defines the maximum number of consecutive failures before the circuit opens. Prevents continuous failures from overwhelming backend services.
- **Reset Timeout**: Configures the time before the circuit attempts to close after being opened. Ensures the circuit remains open long enough for the backend to recover.
- **Half-Open Attempts**: Sets the number of successful requests needed to close the circuit after a failure. Ensures the backend is stable before resuming normal traffic.

### Security Headers

Security headers enhance the security of your web applications by controlling various aspects of HTTP responses.

- **Strict-Transport-Security**: Enforces secure (HTTPS) connections to the server. Prevents man-in-the-middle attacks by ensuring all connections are encrypted.
- **X-Content-Type-Options**: Prevents MIME types from being altered, enhancing security against XSS attacks. Ensures the content is interpreted as intended.
- **X-Frame-Options**: Protects against clickjacking by controlling whether the site can be framed. Prevents malicious sites from embedding your site in iframes.
- **Content-Security-Policy**: Prevents a wide range of attacks, including XSS, by specifying allowed content sources. Controls where resources can be loaded from.
- **X-XSS-Protection**: Enables XSS filtering, providing an additional layer of protection against XSS attacks. Ensures the browser filters out malicious scripts.
- **Referrer-Policy**: Controls the amount of referrer information sent with requests. Protects user privacy by controlling referrer headers.
- **Permissions-Policy**: Controls the permissions and features available to the site. Limits the use of potentially harmful features like geolocation or camera access.
- **Feature-Policy**: (Deprecated, use Permissions-Policy instead) Specifies which browser features are enabled or disabled. Provides similar functionality to Permissions-Policy.

### Bot Detection

Bot detection helps to protect your services from automated traffic, which can be harmful or abusive.

- **Allow List**: Specifies user-agent substrings that are allowed. Ensures legitimate users and bots are not blocked.
- **Deny List**: Specifies user-agent substrings that are denied. Blocks known bad bots and automated traffic.
- **Patterns**: Uses regular expressions to detect bot user agents. Provides flexible and powerful bot detection.
- **Empty User-Agent Handling**: Treats requests with empty user-agent strings as bots, if configured. Blocks suspicious requests without user-agent headers.

### Metrics

Metrics provide insight into the performance and usage of your proxy server, helping to monitor and optimize its operation.

- **Prometheus Integration**: Exposes metrics for monitoring request rates, latencies, error rates, and more. Seamlessly integrates with Prometheus for comprehensive monitoring.
- **Detailed Statistics**: Collects and provides detailed metrics on various aspects of the proxy's operation. Helps to identify bottlenecks and optimize performance.
- **Real-Time Monitoring**: Allows for real-time monitoring and alerting using Prometheus and Grafana. Ensures you can respond quickly to issues and maintain high availability.

### Static File Serving

Static file serving offloads the delivery of static content from your backend servers, improving performance and reducing load.

- **Static File Support**: Serves static files directly from a specified directory, reducing load on backend servers. Ideal for serving images, CSS, JavaScript, and other static assets.
- **Efficient Handling**: Optimized for serving static content with minimal overhead. Ensures fast and efficient delivery of static resources.

## Installation

To install Road7, you need to have Rust and Cargo installed. Then, clone the repository and build the project.

```sh
git clone https://github.com/genc-murat/road7.git
cd road7
cargo build --release
```

## Configuration

The configuration is done through a `config.toml` file. Below is an example configuration file with detailed explanations for each parameter.

```toml
[server]
host = "127.0.0.1"                        # The host address to bind the server to.
port = 8080                               # The port to bind the server to.
max_logging_level = "INFO"                # Maximum logging level (DEBUG, INFO, WARN, ERROR).
pool_size = 10                            # Maximum number of idle connections to maintain.
recv_buffer_size = 8192                   # (Optional) Receive buffer size for the server.
send_buffer_size = 8192                   # (Optional) Send buffer size for the server.

[runtime]
worker_threads = 4                        # Number of worker threads to use for handling requests.

[targets]
[[targets]]
path = "/api"                             # The path prefix that this target handles.
urls = ["http://localhost:3000", "http://localhost:3001"]  # List of backend URLs to forward requests to.
authentication = { 
  type = "JWT", 
  jwt_secret = "your_jwt_secret" 
}                                         # Authentication settings (type and secret for JWT).
retries = { 
  strategy = "ExponentialBackoff", 
  base_delay_seconds = 2, 
  max_attempts = 5 
}                                         # Retry settings (strategy, base delay, and max attempts).
circuit_breaker_config = { 
  max_failures = 3, 
  reset_timeout_seconds = 30, 
  half_open_attempts = 1 
}                                         # Circuit breaker settings (max failures, reset timeout, and half-open attempts).
rate_limiter_config = { 
  type = "TokenBucket", 
  refill_rate = 10, 
  burst_capacity = 20 
}                                         # Rate limiter settings (type, refill rate, and burst capacity).
cache_config = { 
  ttl_seconds = 60, 
  max_size = 100 
}                                         # Cache settings (TTL and max size).
logging_config = { 
  log_requests = true, 
  log_responses = true 
}                                         # Logging settings (log requests and responses).
cors_config = { 
  enabled = true, 
  allow_origin = "*", 
  allow_headers = "*", 
  allow_methods = "GET,POST" 
}                                         # CORS settings (enabled, allowed origin, headers, and methods).

[load_balancer]
algorithm = "RoundRobin"                  # Load balancing algorithm (RoundRobin, Random, LeastConnections, etc.).

[bot_detector]
allow = ["Mozilla"]                       # List of user-agent substrings to allow.
deny = ["Bot"]                            # List of user-agent substrings to deny.
patterns = ["regex_pattern_here"]         # Regular expression patterns to identify bots.
empty_user_agent_is_bot = true            # Whether to treat empty user-agent strings as bots.
```

## Usage

### Running the Server

Run the server with the following command:

```sh
cargo run --release
```

## Modules

### Load Balancer

The load balancer supports multiple algorithms, each suited to different scenarios:

- **RoundRobin**: Distributes requests evenly across all backend servers.
- **Random**: Selects a backend server at random for each request.
- **LeastConnections**: Routes requests to the server with the fewest active connections.
- **WeightedRoundRobin**: Assigns different weights to servers and distributes traffic based on these weights.
- **IPHash**: Uses the client's IP address to consistently route requests to the same server.
- **ConsistentHashing**: Ensures similar requests are routed to the same server, improving cache efficiency.
- **WeightedLeastConnections**: Balances load using both weights and active connections.

### Cache

Road7 offers robust caching capabilities to improve performance and reduce load on backend servers:

- **ttl_seconds**: Time-to-live for cache entries, ensuring data freshness.
- **max_size**: Maximum number of cache entries to control memory usage.
- **serialize**: Optionally serialize cache entries for better performance.

### Authentication

Supports both JWT and Basic authentication to secure access to backend services:

- **JWT**: Validates JSON Web Tokens using a secret key, ensuring secure token-based authentication.
- **Basic**: Validates username and password against a predefined list, providing simple and effective authentication.

### Rate Limiter

Various rate limiting strategies help control traffic and protect backend services:

- **Token Bucket**: Smooths out bursts of traffic by allowing requests to accumulate tokens over time.
- **Leaky Bucket**: Maintains a steady rate of traffic, ideal for consistent load.
- **Fixed Window**: Limits requests within fixed time windows for predictable control.
- **Sliding Log**: Tracks recent request timestamps, providing dynamic rate limiting.
- **Sliding Window**: Combines fixed and sliding window approaches for fine-grained control.
- **Quota**: Sets fixed quotas over periods, ideal for API usage limits.
- **Dynamic**: Adjusts rate limits based on current load, offering adaptive control.

### Retry

Flexible retry strategies ensure resilience in the face of transient failures:

- **FixedInterval**: Retries with a fixed delay between attempts.
- **ExponentialBackoff**: Uses exponentially increasing delays to reduce load during failures.
- **LinearBackoff**: Gradually increases delay between retries for balanced retry attempts.
- **RandomDelay**: Adds randomness to retry delays, preventing thundering herd issues.
- **IncrementalBackoff**: Uses incrementally increasing delays for a predictable retry pattern.
- **FibonacciBackoff**: Follows the Fibonacci sequence for delay intervals, providing a unique retry approach.
- **GeometricBackoff**: Multiplies delays by a constant factor for exponential-like behavior.
- **HarmonicBackoff**: Decreases delays harmonically for balanced retry attempts.
- **JitterBackoff**: Adds jitter to delays, combining other strategies with randomness.

### Circuit Breaker

Prevents backend services from being overwhelmed by managing request failures:

- **max_failures**: Maximum number of consecutive failures before opening the circuit.
- **reset_timeout_seconds**: Time before attempting to reset the circuit after it opens.
- **half_open_attempts**: Number of successful requests needed to close the circuit after a failure.

### Security Headers

Enhances security by adding essential headers to responses:

- **strict_transport_security**: Enforces HTTPS connections.
- **x_content_type_options**: Prevents MIME type sniffing.
- **x_frame_options**: Protects against clickjacking.
- **content_security_policy**: Specifies allowed content sources to prevent XSS attacks.
- **x_xss_protection**: Enables XSS filtering.
- **referrer_policy**: Controls referrer information sent with requests.
- **permissions_policy**: Specifies allowed browser permissions.
- **feature_policy**: (Deprecated, use Permissions-Policy instead) Specifies allowed browser features.

### Bot Detector

Detects and blocks bot requests based on user-agent patterns:

- **allow**: List of user-agent substrings to allow.
- **deny**: List of user-agent substrings to deny.
- **patterns**: Regular expression patterns to identify bots.
- **empty_user_agent_is_bot**: Treats requests with empty user-agent strings as bots if enabled.

### Metrics

Integrates with Prometheus for detailed monitoring and analytics:

- **Prometheus Integration**: Exposes metrics for monitoring request rates, latencies, and error rates.
- **Detailed Statistics**: Collects and provides detailed metrics on various aspects of the proxy's operation.
- **Real-Time Monitoring**: Allows for real-time monitoring and alerting using Prometheus and Grafana.

### Static File Serving

Static file serving offloads the delivery of static content from your backend servers, improving performance and reducing load.

- **Static File Support**: Serves static files directly from a specified directory, reducing load on backend servers. Ideal for serving images, CSS, JavaScript, and other static assets.
- **Efficient Handling**: Optimized for serving static content with minimal overhead. Ensures fast and efficient delivery of static resources.

## Why Choose Road7?

Road7 is designed to provide a comprehensive solution for modern web applications, offering advanced features and robust performance. Here’s why you should consider using Road7:

- **Flexibility**: Road7’s modular architecture allows you to customize and extend its functionality to meet your specific needs.
- **Performance**: Built with Rust, Road7 is optimized for high performance, ensuring low latency and efficient resource usage.
- **Scalability**: Road7 handles high traffic loads with ease, making it suitable for both small applications and large-scale enterprise deployments.
- **Security**: Advanced security features, including authentication, rate limiting, circuit breaking, and security headers, ensure your applications are protected.
- **Ease of Use**: Road7 is easy to configure and deploy

, with clear documentation and comprehensive configuration options.
- **Community and Support**: Road7 is an open-source project with an active community. Contributions are welcome, and support is readily available.

With its rich feature set and robust architecture, Road7 is the ideal choice for developers and enterprises looking to implement a scalable, secure, and high-performance proxy solution.