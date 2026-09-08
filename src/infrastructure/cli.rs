use std::path::PathBuf;

use serde::Deserialize;

const DEFAULT_LISTEN: &str = "0.0.0.0:8080";
const DEFAULT_METRICS_LISTEN: &str = "0.0.0.0:9090";

pub struct CommandLineOptions {
    pub listen: String,
    pub target: String,
    pub log_path: Option<PathBuf>,
    pub metrics_listen: String,
    pub console: bool,
    pub insecure_target_tls: bool,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    pub dns_servers: Vec<String>,
}

#[derive(Default, Deserialize)]
struct FileConfiguration {
    listen: Option<String>,
    target: Option<String>,
    log: Option<PathBuf>,
    metrics_listen: Option<String>,
    console: Option<bool>,
    insecure_target_tls: Option<bool>,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    dns_servers: Option<Vec<String>>,
}

#[derive(Default)]
struct CommandLineOverrides {
    listen: Option<String>,
    target: Option<String>,
    log_path: Option<PathBuf>,
    metrics_listen: Option<String>,
    console: Option<bool>,
    insecure_target_tls: bool,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    dns_servers: Option<Vec<String>>,
}

impl CommandLineOptions {
    pub fn parse() -> Self {
        let (config_path, overrides) = parse_arguments();
        let file_configuration = config_path
            .as_deref()
            .map(load_file_configuration)
            .unwrap_or_default();

        let options = Self {
            listen: overrides
                .listen
                .or(file_configuration.listen)
                .unwrap_or_else(|| DEFAULT_LISTEN.to_string()),
            target: overrides
                .target
                .or(file_configuration.target)
                .unwrap_or_default(),
            log_path: overrides.log_path.or(file_configuration.log),
            metrics_listen: overrides
                .metrics_listen
                .or(file_configuration.metrics_listen)
                .unwrap_or_else(|| DEFAULT_METRICS_LISTEN.to_string()),
            console: overrides
                .console
                .or(file_configuration.console)
                .unwrap_or(true),
            insecure_target_tls: overrides.insecure_target_tls
                || file_configuration.insecure_target_tls.unwrap_or(false),
            tls_cert: overrides.tls_cert.or(file_configuration.tls_cert),
            tls_key: overrides.tls_key.or(file_configuration.tls_key),
            dns_servers: overrides
                .dns_servers
                .or(file_configuration.dns_servers)
                .unwrap_or_default(),
        };

        validate_options(options)
    }
}

fn parse_arguments() -> (Option<PathBuf>, CommandLineOverrides) {
    let mut arguments = std::env::args().skip(1);
    let mut config_path = None;
    let mut overrides = CommandLineOverrides::default();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--config" => config_path = Some(PathBuf::from(next_value(&mut arguments, "--config"))),
            "--listen" => overrides.listen = Some(next_value(&mut arguments, "--listen")),
            "--target" => overrides.target = Some(next_value(&mut arguments, "--target")),
            "--log" => {
                overrides.log_path = Some(PathBuf::from(next_value(&mut arguments, "--log")))
            }
            "--metrics-listen" => {
                overrides.metrics_listen = Some(next_value(&mut arguments, "--metrics-listen"))
            }
            "--console" => {
                overrides.console = Some(
                    arguments
                        .next()
                        .map(|value| value != "false")
                        .unwrap_or(true),
                )
            }
            "--insecure-target-tls" => overrides.insecure_target_tls = true,
            "--tls-cert" => {
                overrides.tls_cert = Some(PathBuf::from(next_value(&mut arguments, "--tls-cert")))
            }
            "--tls-key" => {
                overrides.tls_key = Some(PathBuf::from(next_value(&mut arguments, "--tls-key")))
            }
            "--dns-servers" => {
                overrides.dns_servers = Some(
                    next_value(&mut arguments, "--dns-servers")
                        .split(',')
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .collect(),
                )
            }
            other => eprintln!("ignoring unknown argument: {}", other),
        }
    }

    (config_path, overrides)
}

fn load_file_configuration(path: &std::path::Path) -> FileConfiguration {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read config file {}: {}", path.display(), error));
    toml::from_str(&contents)
        .unwrap_or_else(|error| panic!("cannot parse config file {}: {}", path.display(), error))
}

fn validate_options(options: CommandLineOptions) -> CommandLineOptions {
    if options.target.is_empty() {
        eprintln!("missing required --target option or config value");
        std::process::exit(1);
    }
    if options.tls_cert.is_some() != options.tls_key.is_some() {
        eprintln!("tls_cert/tls_key or --tls-cert/--tls-key must be provided together");
        std::process::exit(1);
    }

    options
}

fn next_value(arguments: &mut impl Iterator<Item = String>, option: &str) -> String {
    arguments
        .next()
        .unwrap_or_else(|| panic!("{} requires a value", option))
}
