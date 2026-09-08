# TrafficSniffer

## Goals
A small tool to help the migration process by collecting information about traffic.

Useful when:
- There is no clear identification of application/system users

Benefits:
- List of devices that use the URLs

## How it works

TrafficSniffer is a lightweight reverse proxy written in Rust. Users keep
hitting the old URLs in their browser; the proxy captures request metadata
(client IP, client hostname, method, host, path, query, ports, user-agent,
referer, status code, duration) and forwards the call unchanged to the legacy
application running on another server/port. Each request is logged as one
JSON line (JSONL), optionally to a log file and to stdout. Prometheus metrics
are exposed on a separate port by default.

## Architecture

The source is organized using a lightweight Domain-Driven Design structure:

```text
src/
|-- main.rs                         Composition root
|-- domain/
|   |-- config.rs                   Proxy configuration
|   `-- traffic.rs                  TrafficRecord business event
|-- application/
|   `-- proxy_service.rs            Request forwarding use case
`-- infrastructure/
    |-- cli.rs                      Command-line adapter
    |-- server.rs                   TCP, HTTP and incoming HTTPS adapter
    |-- tls.rs                      Incoming and outgoing TLS adapters
    `-- traffic_log.rs              JSONL file and console adapter
```

Responsibilities are intentionally separated:

- **Domain** contains the data that describes the migration traffic, without
  knowing how it is transported or stored.
- **Application** implements `ProxyService::forward_request`: it extracts
  request metadata, builds the legacy target URI, forwards the request and
  creates a `TrafficRecord`.
- **Infrastructure** connects the use case to the outside world: command-line
  arguments, TCP/HTTP/TLS connections and JSONL logging.
- **`main.rs`** only composes these components and starts the listener.

For each request, the flow is:

```text
Browser
  -> HTTP or HTTPS listener
  -> ProxyService::forward_request
  -> legacy application
  -> TrafficRecord
  -> TrafficLogger (JSONL file and optional stdout)
```

This keeps the proxy behavior readable in the application layer while leaving
TLS, Hyper, Tokio and filesystem details in infrastructure modules.

## Build

```powershell
cargo build --release
```

The binary is generated at `target/release/trafficsniffer.exe` (Windows) or
`target/release/trafficsniffer` (Linux/macOS) — a single self-contained
executable, easy to copy to a server.

## Run

```powershell
./trafficsniffer --listen 0.0.0.0:8080 --target http://old-app:9090
```

The same settings can be provided in a TOML configuration file:

```toml
listen = "0.0.0.0:8081"
target = "https://localhost:4044/"
log = "traffic.log"
metrics_listen = "0.0.0.0:9090"
console = true
insecure_target_tls = true
tls_cert = "certs/traffic-sniffer.crt"
tls_key = "certs/traffic-sniffer.key"
```

Start the application with:

```powershell
./trafficsniffer --config traffic-sniffer.toml
```

All CLI settings have an equivalent TOML key. CLI arguments override values
from the configuration file, and omitted values use the defaults. The
`target` value is required either in the file or on the command line.

Prometheus metrics are available by default at:

```text
http://localhost:9090/metrics
```

The request counter is exposed as
`trafficsniffer_requests_total{path="...",client_ip="..."}`. It counts each
proxied HTTP request grouped by request path and client IP.

Accepted TCP connections are exposed as
`trafficsniffer_tcp_connections_total{client_ip,listen_port,transport}`.
This counts TCP connections accepted by the proxy and labels them with the
source endpoint and `http` or `https` transport. Raw TCP connections do not
have an HTTP path; the existing proxy still expects HTTP after the TCP/TLS
connection is accepted and does not forward arbitrary TCP protocols.

To write JSONL traffic logs to a file, provide the optional `--log` argument:

```powershell
./trafficsniffer --target http://old-app:9090 --log traffic.log
```

Each JSONL entry includes a `client_hostname` field, resolved from the
client's IP address via reverse DNS (PTR record lookup), similar to running
`nslookup`. The resolver uses the system's DNS configuration, caches results
per IP for the lifetime of the process, and falls back to `"unknown"` when
the lookup fails, times out (500 ms), or no PTR record exists.

To use specific DNS servers instead of the system configuration, provide
`--dns-servers` (comma-separated) or the `dns_servers` TOML array. Each
configured server is tried in turn, one attempt per server, until one
answers:

```powershell
./trafficsniffer --target http://old-app:9090 --log traffic.log --dns-servers 10.0.0.1,10.0.0.2
```

The legacy target can also use HTTPS. The proxy validates its certificate
against the server's system certificate store:

```powershell
./trafficsniffer --listen 0.0.0.0:8080 --target https://old-app:9443 --log traffic.log
```

For a legacy server with a self-signed or otherwise untrusted certificate,
disable target certificate validation explicitly:

```powershell
./trafficsniffer --listen 0.0.0.0:8080 --target https://localhost:4044/ --insecure-target-tls --log traffic.log
```

To accept HTTPS connections from browsers, provide the server certificate and
its private key. The certificate is presented to the browser; client
certificates are not required or validated:

```powershell
./trafficsniffer --listen 0.0.0.0:8080 --target https://localhost:4044/ `
  --tls-cert C:\certs\traffic-sniffer.crt `
  --tls-key C:\certs\traffic-sniffer.key `
  --insecure-target-tls --log traffic.log
```

For local development, a self-signed certificate for `localhost` can be
generated with:

```powershell
New-Item -ItemType Directory -Force certs
openssl req -x509 -nodes -newkey rsa:2048 `
  -keyout certs\traffic-sniffer.key `
  -out certs\traffic-sniffer.crt `
  -days 365 -subj "/CN=localhost" `
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
```

With these options, users must browse to `https://<hostname>:8080`. Without
them, the listener remains plain HTTP and users must browse to
`http://<hostname>:8080`.

Options:
- `--config` : path to the TOML configuration file
- `--listen` : address/port to listen on (default `0.0.0.0:8080`)
- `--target` : URL of the legacy application to forward to (required)
- `--metrics-listen` : address/port for the Prometheus endpoint (default `0.0.0.0:9090`)
- `--log` : optional path to the JSONL file where captured traffic is appended
- `--console` : also print captured traffic to stdout (default `true`)
- `--insecure-target-tls` : do not validate the legacy target's HTTPS certificate
- `--tls-cert` : PEM certificate presented for incoming HTTPS (requires `--tls-key`)
- `--tls-key` : PEM private key for incoming HTTPS (requires `--tls-cert`)
- `--dns-servers` : comma-separated list of DNS server IPs to use for reverse hostname lookups instead of the system configuration

The metrics endpoint is plain HTTP and should normally be restricted to the
monitoring network. The `client_ip` label is intentionally high-cardinality;
for large or untrusted traffic volumes, consider protecting or aggregating
the endpoint before exposing it broadly.


Then point the old URLs' DNS/hosts entry (or load balancer) at this proxy
instead of the legacy application directly.

## Alternatives considered

- **Traefik (containerized)**: Traefik's access logs already capture most of
  this data (client IP, path, status, duration) and it can forward to the
  legacy backend as a regular reverse proxy. It's a solid option if a
  container engine is already available, but it adds an extra piece of
  infrastructure to deploy/operate compared to a single static binary.
- **Go**: an equally viable option (also compiles to a single binary), but
  Rust was chosen here.

