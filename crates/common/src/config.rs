//! Config primitives and placeholder loading.
//! Full config schema will be expanded as features are implemented.

use serde::Deserialize;

/// Job source mode: direct RPC (getblocktemplate) or node REST API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobSourceMode {
    /// Direct JSON-RPC getblocktemplate.
    #[default]
    Rpc,
    /// Node REST API GET /v1/az/mining/template/current.
    Api,
}

impl std::fmt::Display for JobSourceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobSourceMode::Rpc => write!(f, "rpc"),
            JobSourceMode::Api => write!(f, "api"),
        }
    }
}

impl<'de> Deserialize<'de> for JobSourceMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "rpc" => Ok(JobSourceMode::Rpc),
            "api" => Ok(JobSourceMode::Api),
            _ => Err(serde::de::Error::custom(format!(
                "invalid job_source_mode '{}', expected 'rpc' or 'api'",
                s
            ))),
        }
    }
}

/// Root pool configuration. Placeholder for bootstrap.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PoolConfig {
    #[serde(default)]
    pub pool: PoolSection,

    #[serde(default)]
    pub api: ApiSection,

    #[serde(default)]
    pub stratum: StratumSection,

    #[serde(default)]
    pub daemon: DaemonSection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolSection {
    #[serde(default = "default_pool_name")]
    pub name: String,
}

impl Default for PoolSection {
    fn default() -> Self {
        Self {
            name: default_pool_name(),
        }
    }
}

fn default_pool_name() -> String {
    "azcoin-pool".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiSection {
    #[serde(default = "default_api_bind")]
    pub bind: String,

    #[serde(default = "default_api_port")]
    pub port: u16,
}

impl Default for ApiSection {
    fn default() -> Self {
        Self {
            bind: default_api_bind(),
            port: default_api_port(),
        }
    }
}

fn default_api_bind() -> String {
    "0.0.0.0".to_string()
}

fn default_api_port() -> u16 {
    8080
}

#[derive(Debug, Clone, Deserialize)]
pub struct StratumSection {
    #[serde(default = "default_stratum_bind")]
    pub bind: String,

    #[serde(default = "default_stratum_port")]
    pub port: u16,
}

impl Default for StratumSection {
    fn default() -> Self {
        Self {
            bind: default_stratum_bind(),
            port: default_stratum_port(),
        }
    }
}

fn default_stratum_bind() -> String {
    "0.0.0.0".to_string()
}

fn default_stratum_port() -> u16 {
    3333
}

#[derive(Debug, Clone, Deserialize)]
pub struct DaemonSection {
    #[serde(default = "default_daemon_url")]
    pub url: String,

    /// RPC username for JSON-RPC auth. Empty = no auth.
    #[serde(default)]
    pub rpc_user: String,

    /// RPC password for JSON-RPC auth.
    #[serde(default)]
    pub rpc_password: String,

    /// Job source: "rpc" (getblocktemplate) or "api" (GET /v1/az/mining/template/current).
    #[serde(default)]
    pub job_source_mode: JobSourceMode,
}

impl Default for DaemonSection {
    fn default() -> Self {
        Self {
            url: default_daemon_url(),
            rpc_user: String::new(),
            rpc_password: String::new(),
            job_source_mode: JobSourceMode::default(),
        }
    }
}

fn default_daemon_url() -> String {
    "http://127.0.0.1:8332".to_string()
}

/// Load config from file or return defaults. Placeholder implementation.
pub fn load_config(path: Option<&str>) -> Result<PoolConfig, crate::PoolError> {
    let path = path.unwrap_or("config.toml");
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let config: PoolConfig =
                toml::from_str(&s).map_err(|e| crate::PoolError::Config(e.to_string()))?;
            Ok(config)
        }
        Err(_) => Ok(PoolConfig::default()),
    }
}
