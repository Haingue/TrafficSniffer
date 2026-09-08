use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use hickory_resolver::config::{NameServerConfig, ResolverConfig};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::RData;
use hickory_resolver::{Resolver, TokioResolver};
use tokio::sync::Mutex;

const UNKNOWN_HOSTNAME: &str = "unknown";
const LOOKUP_TIMEOUT: Duration = Duration::from_millis(500);

/// Resolves client IPs to hostnames via reverse DNS (PTR records), caching
/// results so repeated requests from the same IP do not incur a DNS round trip.
#[derive(Clone)]
pub struct HostnameResolver {
    resolver: Option<Arc<TokioResolver>>,
    cache: Arc<Mutex<HashMap<IpAddr, String>>>,
}

impl HostnameResolver {
    /// `dns_servers` overrides the system DNS configuration; each is tried in
    /// turn (one attempt per server) until one answers. Resolution is disabled
    /// entirely when no DNS server is configured.
    pub fn new(dns_servers: &[String]) -> Self {
        let resolver = if dns_servers.is_empty() {
            None
        } else {
            build_resolver(dns_servers)
                .map(Arc::new)
                .map_err(|error| eprintln!("failed to initialize DNS resolver: {}", error))
                .ok()
        };

        Self {
            resolver,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn resolve(&self, ip: IpAddr) -> String {
        if let Some(hostname) = self.cache.lock().await.get(&ip) {
            return hostname.clone();
        }

        let hostname = self.reverse_lookup(ip).await;
        self.cache.lock().await.insert(ip, hostname.clone());
        hostname
    }

    async fn reverse_lookup(&self, ip: IpAddr) -> String {
        let Some(resolver) = &self.resolver else {
            return UNKNOWN_HOSTNAME.to_string();
        };

        match tokio::time::timeout(LOOKUP_TIMEOUT, resolver.reverse_lookup(ip)).await {
            Ok(Ok(lookup)) => lookup
                .answers()
                .iter()
                .find_map(|record| match &record.data {
                    RData::PTR(name) => Some(name.to_string().trim_end_matches('.').to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| UNKNOWN_HOSTNAME.to_string()),
            _ => UNKNOWN_HOSTNAME.to_string(),
        }
    }
}

fn build_resolver(dns_servers: &[String]) -> Result<TokioResolver, String> {
    let ips: Vec<IpAddr> = dns_servers
        .iter()
        .map(|server| {
            server
                .parse()
                .map_err(|_| format!("invalid DNS server address: {}", server))
        })
        .collect::<Result<_, _>>()?;

    let name_servers = ips.into_iter().map(NameServerConfig::udp_and_tcp).collect();
    let config = ResolverConfig::from_name_servers(name_servers);
    let mut builder = Resolver::builder_with_config(config, TokioRuntimeProvider::default());
    // one attempt per configured DNS server before moving on to the next
    builder.options_mut().attempts = 1;

    builder.build().map_err(|error| error.to_string())
}
