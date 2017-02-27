//! # Config module.
//!
//! The Config module provides the beamium configuration.
//! It set defaults and then load config from '/etc', local dir and provided path.
use std::fs::File;
use std::io::Read;
use std::io;
use std::fmt;
use std::string::String;
use std::path::Path;
use std::error;
use std::error::Error;
use yaml_rust::{YamlLoader, ScanError};
use cast;
use std::collections::HashMap;
use regex;
use slog;

#[derive(Debug)]
#[derive(Clone)]
/// Config root.
pub struct Config {
    pub sources: Vec<Source>,
    pub sinks: Vec<Sink>,
    pub labels: HashMap<String, String>,
    pub parameters: Parameters,
}

#[derive(Debug)]
#[derive(Clone)]
/// Source config.
pub struct Source {
    pub name: String,
    pub url: String,
    pub period: u64,
    pub format: SourceFormat,
    pub metrics: Option<regex::RegexSet>,
}

#[derive(Debug)]
#[derive(Clone)]
/// Source format.
pub enum SourceFormat {
    Prometheus,
    Sensision,
}

#[derive(Debug)]
#[derive(Clone)]
/// Sink config.
pub struct Sink {
    pub name: String,
    pub url: String,
    pub token: String,
    pub token_header: String,
    pub selector: Option<regex::Regex>,
    pub ttl: u64,
    pub size: u64,
}

#[derive(Debug)]
#[derive(Clone)]
/// Parameters config.
pub struct Parameters {
    pub scan_period: u64,
    pub sink_dir: String,
    pub source_dir: String,
    pub batch_size: u64,
    pub batch_count: u64,
    pub log_file: String,
    pub log_level: slog::Level,
    pub timeout: u64,
}

#[derive(Debug)]
/// Config Error.
pub enum ConfigError {
    Io(io::Error),
    Yaml(ScanError),
    Regex(regex::Error),
    Format(Box<Error>),
}

impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> ConfigError {
        ConfigError::Io(err)
    }
}
impl From<ScanError> for ConfigError {
    fn from(err: ScanError) -> ConfigError {
        ConfigError::Yaml(err)
    }
}
impl From<regex::Error> for ConfigError {
    fn from(err: regex::Error) -> ConfigError {
        ConfigError::Regex(err)
    }
}
impl From<Box<Error>> for ConfigError {
    fn from(err: Box<Error>) -> ConfigError {
        ConfigError::Format(err)
    }
}
impl<'a> From<&'a str> for ConfigError {
    fn from(err: &str) -> ConfigError {
        ConfigError::Format(From::from(err))
    }
}
impl From<String> for ConfigError {
    fn from(err: String) -> ConfigError {
        ConfigError::Format(From::from(err))
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ConfigError::Io(ref err) => err.fmt(f),
            ConfigError::Yaml(ref err) => err.fmt(f),
            ConfigError::Regex(ref err) => err.fmt(f),
            ConfigError::Format(ref err) => err.fmt(f),
        }
    }
}

impl error::Error for ConfigError {
    fn description(&self) -> &str {
        match *self {
            ConfigError::Io(ref err) => err.description(),
            ConfigError::Yaml(ref err) => err.description(),
            ConfigError::Regex(ref err) => err.description(),
            ConfigError::Format(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            ConfigError::Io(ref err) => Some(err),
            ConfigError::Yaml(ref err) => Some(err),
            ConfigError::Regex(ref err) => Some(err),
            ConfigError::Format(ref err) => Some(err.as_ref()),
        }
    }
}

/// Load config.
///
/// Setup a defaults config and then load config from '/etc', local dir and provided path.
/// Return Err if provided path is not found or if config is unprocessable.
pub fn load_config(config_path: &str) -> Result<Config, ConfigError> {
    // Defaults
    let mut config = Config {
        sources: Vec::new(),
        labels: HashMap::new(),
        sinks: Vec::new(),
        parameters: Parameters {
            scan_period: 1000,
            sink_dir: String::from("sinks"),
            source_dir: String::from("sources"),
            batch_size: 200000,
            batch_count: 250,
            log_file: String::from(env!("CARGO_PKG_NAME")) + ".log",
            log_level: slog::Level::Info,
            timeout: 300,
        },
    };

    if config_path.is_empty() {
        // Load from etc
        if Path::new("/etc/beamium/config.yaml").exists() {
            try!(load_path("/etc/beamium/config.yaml", &mut config));
        }

        // Load local
        if Path::new("config.yaml").exists() {
            try!(load_path("config.yaml", &mut config));
        }
    } else {
        // Load from provided path
        try!(load_path(config_path, &mut config));
    }

    Ok(config)
}

/// Extend confif from file.
fn load_path<P: AsRef<Path>>(file_path: P, config: &mut Config) -> Result<(), ConfigError> {
    let mut file = try!(File::open(file_path));
    let mut contents = String::new();
    try!(file.read_to_string(&mut contents));
    let docs = try!(YamlLoader::load_from_str(&contents));

    for doc in &docs {
        if !doc["sources"].is_badvalue() {
            let sources = try!(doc["sources"]
                .as_hash()
                .ok_or("sources should be a map"));

            for (k, v) in sources {
                let name = try!(k.as_str()
                    .ok_or("sources keys should be a string"));
                let url = try!(v["url"]
                    .as_str()
                    .ok_or(format!("sources.{}.url is required and should be a string", name)));
                let period = try!(v["period"]
                    .as_i64()
                    .ok_or(format!("sources.{}.period is required and should be a number", name)));
                let period = try!(cast::u64(period)
                    .map_err(|_| format!("sources.{}.period is invalid", name)));
                let format = if v["format"].is_badvalue() {
                    SourceFormat::Prometheus
                } else {
                    let f = try!(v["format"]
                        .as_str()
                        .ok_or(format!("sinks.{}.format should be a string", name)));

                    if f == "prometheus" {
                        SourceFormat::Prometheus
                    } else if f == "sensision" {
                        SourceFormat::Sensision
                    } else {
                        return Err(format!("sinks.{}.format should be 'Prometheus' or 'sensision'",
                                           name)
                            .into());
                    }
                };
                let metrics = if v["metrics"].is_badvalue() {
                    None
                } else {
                    let mut metrics = Vec::new();
                    let values = try!(v["metrics"].as_vec().ok_or("metrics should be an array"));
                    for v in values {
                        let value = try!(regex::Regex::new(try!(v.as_str()
                            .ok_or(format!("metrics.{} is invalid", name)))));
                        metrics.push(String::from(r"^(\S*)\s") + value.as_str());
                    }

                    Some(try!(regex::RegexSet::new(&metrics)))
                };

                config.sources.push(Source {
                    name: String::from(name),
                    url: String::from(url),
                    period: period,
                    format: format,
                    metrics: metrics,
                })
            }
        }

        if !doc["sinks"].is_badvalue() {
            let sinks = try!(doc["sinks"].as_hash().ok_or("sinks should be a map"));
            for (k, v) in sinks {
                let name = try!(k.as_str().ok_or("sinks keys should be a string"));
                let url = try!(v["url"]
                    .as_str()
                    .ok_or(format!("sinks.{}.url is required and should be a string", name)));
                let token = try!(v["token"]
                    .as_str()
                    .ok_or(format!("sinks.{}.token is required and should be a string", name)));
                let token_header = if v["token-header"].is_badvalue() {
                    "X-Warp10-Token"
                } else {
                    try!(v["token-header"]
                        .as_str()
                        .ok_or(format!("sinks.{}.token-header should be a string", name)))
                };

                let selector = if v["selector"].is_badvalue() {
                    None
                } else {
                    Some(try!(regex::Regex::new(format!("^{}",
                                                        try!(v["selector"]
                                                            .as_str()
                                                            .ok_or(format!("sinks.{}.selector \
                                                                            is invalid",
                                                                           name))))
                        .as_str())))
                };

                let ttl = if v["ttl"].is_badvalue() {
                    3600
                } else {
                    let ttl = try!(v["ttl"]
                        .as_i64()
                        .ok_or(format!("sinks.{}.ttl should be a number", name)));
                    try!(cast::u64(ttl)
                        .map_err(|_| format!("sinks.{}.ttl should be a positive number", name)))
                };

                let size = if v["size"].is_badvalue() {
                    1073741824
                } else {
                    let size = try!(v["size"]
                        .as_i64()
                        .ok_or(format!("sinks.{}.size should be a number", name)));
                    try!(cast::u64(size)
                        .map_err(|_| format!("sinks.{}.size should be a positive number", name)))
                };

                config.sinks.push(Sink {
                    name: String::from(name),
                    url: String::from(url),
                    token: String::from(token),
                    token_header: String::from(token_header),
                    selector: selector,
                    ttl: ttl,
                    size: size,
                })
            }
        }

        if !doc["labels"].is_badvalue() {
            let labels = try!(doc["labels"].as_hash().ok_or("labels should be a map"));
            for (k, v) in labels {
                let name = try!(k.as_str().ok_or("labels keys should be a string"));
                let value = try!(v.as_str()
                    .ok_or(format!("labels.{} value should be a string", name)));
                config.labels.insert(String::from(name), String::from(value));
            }
        }

        if !doc["parameters"].is_badvalue() {
            if !doc["parameters"]["source-dir"].is_badvalue() {
                let source_dir = try!(doc["parameters"]["source-dir"]
                    .as_str()
                    .ok_or(format!("parameters.source-dir should be a string")));
                config.parameters.source_dir = String::from(source_dir);
            }

            if !doc["parameters"]["sink-dir"].is_badvalue() {
                let sink_dir = try!(doc["parameters"]["sink-dir"]
                    .as_str()
                    .ok_or(format!("parameters.sink-dir should be a string")));
                config.parameters.sink_dir = String::from(sink_dir);
            }

            if !doc["parameters"]["scan-period"].is_badvalue() {
                let scan_period = try!(doc["parameters"]["scan-period"]
                    .as_i64()
                    .ok_or(format!("parameters.scan-period should be a number")));
                let scan_period = try!(cast::u64(scan_period)
                    .map_err(|_| format!("parameters.scan-period is invalid")));
                config.parameters.scan_period = scan_period;
            }

            if !doc["parameters"]["batch-size"].is_badvalue() {
                let batch_size = try!(doc["parameters"]["batch-size"]
                    .as_i64()
                    .ok_or(format!("parameters.batch-size should be a number")));
                let batch_size = try!(cast::u64(batch_size)
                    .map_err(|_| format!("parameters.batch-size is invalid")));
                config.parameters.batch_size = batch_size;
            }

            if !doc["parameters"]["batch-count"].is_badvalue() {
                let batch_count = try!(doc["parameters"]["batch-count"]
                    .as_i64()
                    .ok_or(format!("parameters.batch-count should be a number")));
                let batch_count = try!(cast::u64(batch_count)
                    .map_err(|_| format!("parameters.batch-count is invalid")));
                config.parameters.batch_count = batch_count;
            }

            if !doc["parameters"]["log-file"].is_badvalue() {
                let log_file = try!(doc["parameters"]["log-file"]
                    .as_str()
                    .ok_or(format!("parameters.log-file should be a string")));
                config.parameters.log_file = String::from(log_file);
            }

            if !doc["parameters"]["log-level"].is_badvalue() {
                let log_level = try!(doc["parameters"]["log-level"]
                    .as_i64()
                    .ok_or(format!("parameters.log-level should be a number")));
                let log_level = try!(cast::u64(log_level)
                    .map_err(|_| format!("parameters.log-level is invalid")));
                let log_level = try!(slog::Level::from_usize(log_level as usize)
                    .ok_or(format!("parameters.log-level is invalid")));
                config.parameters.log_level = log_level;
            }

            if !doc["parameters"]["timeout"].is_badvalue() {
                let timeout = try!(doc["parameters"]["timeout"]
                    .as_i64()
                    .ok_or(format!("parameters.timeout should be a number")));
                let timeout = try!(cast::u64(timeout)
                    .map_err(|_| format!("parameters.timeout is invalid")));
                config.parameters.timeout = timeout;
            }
        }
    }

    Ok(())
}
