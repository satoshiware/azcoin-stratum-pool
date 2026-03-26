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

    #[serde(default = "default_pool_initial_difficulty")]
    pub initial_difficulty: u32,

    #[serde(default)]
    pub payout_script_pubkey_hex: String,
}

impl Default for PoolSection {
    fn default() -> Self {
        Self {
            name: default_pool_name(),
            initial_difficulty: default_pool_initial_difficulty(),
            payout_script_pubkey_hex: String::new(),
        }
    }
}

impl PoolSection {
    pub fn payout_script_pubkey_bytes(&self) -> Result<Option<Vec<u8>>, crate::PoolError> {
        let hex = self.payout_script_pubkey_hex.trim();
        if hex.is_empty() {
            return Ok(None);
        }

        hex::decode(hex).map(Some).map_err(|e| {
            crate::PoolError::Config(format!("invalid pool.payout_script_pubkey_hex: {}", e))
        })
    }
}

fn default_pool_name() -> String {
    "azcoin-pool".to_string()
}

fn default_pool_initial_difficulty() -> u32 {
    1
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

    /// Bearer token for Node API mode. Empty = no auth.
    #[serde(default)]
    pub node_api_token: String,
}

impl Default for DaemonSection {
    fn default() -> Self {
        Self {
            url: default_daemon_url(),
            rpc_user: String::new(),
            rpc_password: String::new(),
            job_source_mode: JobSourceMode::default(),
            node_api_token: String::new(),
        }
    }
}

fn default_daemon_url() -> String {
    "http://127.0.0.1:8332".to_string()
}

/// Environment variable names for overrides. Use double underscore for nested keys.
/// Example: DAEMON__JOB_SOURCE_MODE=api overrides daemon.job_source_mode.
pub mod env_keys {
    pub const POOL_PAYOUT_SCRIPT_PUBKEY_HEX: &str = "POOL__PAYOUT_SCRIPT_PUBKEY_HEX";
    pub const DAEMON_JOB_SOURCE_MODE: &str = "DAEMON__JOB_SOURCE_MODE";
    pub const DAEMON_URL: &str = "DAEMON__URL";
    pub const DAEMON_RPC_USER: &str = "DAEMON__RPC_USER";
    pub const DAEMON_RPC_PASSWORD: &str = "DAEMON__RPC_PASSWORD";
    pub const DAEMON_NODE_API_TOKEN: &str = "DAEMON__NODE_API_TOKEN";
    pub const API_BIND: &str = "API__BIND";
    pub const API_PORT: &str = "API__PORT";
    pub const STRATUM_BIND: &str = "STRATUM__BIND";
    pub const STRATUM_PORT: &str = "STRATUM__PORT";
}

/// Apply environment variable overrides to config. Env vars take precedence over TOML.
/// Uses double-underscore convention for nested keys (e.g. DAEMON__URL -> daemon.url).
pub fn apply_env_overrides(config: &mut PoolConfig) {
    if let Ok(v) = std::env::var(env_keys::POOL_PAYOUT_SCRIPT_PUBKEY_HEX) {
        config.pool.payout_script_pubkey_hex = v.trim().to_string();
    }
    if let Ok(v) = std::env::var(env_keys::DAEMON_JOB_SOURCE_MODE) {
        if let Ok(mode) = parse_job_source_mode(&v) {
            config.daemon.job_source_mode = mode;
        }
    }
    if let Ok(v) = std::env::var(env_keys::DAEMON_URL) {
        config.daemon.url = v.trim().to_string();
    }
    if let Ok(v) = std::env::var(env_keys::DAEMON_RPC_USER) {
        config.daemon.rpc_user = v;
    }
    if let Ok(v) = std::env::var(env_keys::DAEMON_RPC_PASSWORD) {
        config.daemon.rpc_password = v;
    }
    if let Ok(v) = std::env::var(env_keys::DAEMON_NODE_API_TOKEN) {
        config.daemon.node_api_token = v;
    }
    if let Ok(v) = std::env::var(env_keys::API_BIND) {
        config.api.bind = v.trim().to_string();
    }
    if let Ok(v) = std::env::var(env_keys::API_PORT) {
        if let Ok(port) = v.trim().parse::<u16>() {
            config.api.port = port;
        }
    }
    if let Ok(v) = std::env::var(env_keys::STRATUM_BIND) {
        config.stratum.bind = v.trim().to_string();
    }
    if let Ok(v) = std::env::var(env_keys::STRATUM_PORT) {
        if let Ok(port) = v.trim().parse::<u16>() {
            config.stratum.port = port;
        }
    }
}

/// Apply overrides from a map. Used for testing without touching process env.
pub fn apply_env_overrides_from(config: &mut PoolConfig, env: &impl Fn(&str) -> Option<String>) {
    if let Some(v) = env(env_keys::POOL_PAYOUT_SCRIPT_PUBKEY_HEX) {
        config.pool.payout_script_pubkey_hex = v.trim().to_string();
    }
    if let Some(v) = env(env_keys::DAEMON_JOB_SOURCE_MODE) {
        if let Ok(mode) = parse_job_source_mode(&v) {
            config.daemon.job_source_mode = mode;
        }
    }
    if let Some(v) = env(env_keys::DAEMON_URL) {
        config.daemon.url = v.trim().to_string();
    }
    if let Some(v) = env(env_keys::DAEMON_RPC_USER) {
        config.daemon.rpc_user = v;
    }
    if let Some(v) = env(env_keys::DAEMON_RPC_PASSWORD) {
        config.daemon.rpc_password = v;
    }
    if let Some(v) = env(env_keys::DAEMON_NODE_API_TOKEN) {
        config.daemon.node_api_token = v;
    }
    if let Some(v) = env(env_keys::API_BIND) {
        config.api.bind = v.trim().to_string();
    }
    if let Some(v) = env(env_keys::API_PORT) {
        if let Ok(port) = v.trim().parse::<u16>() {
            config.api.port = port;
        }
    }
    if let Some(v) = env(env_keys::STRATUM_BIND) {
        config.stratum.bind = v.trim().to_string();
    }
    if let Some(v) = env(env_keys::STRATUM_PORT) {
        if let Ok(port) = v.trim().parse::<u16>() {
            config.stratum.port = port;
        }
    }
}

fn parse_job_source_mode(s: &str) -> Result<JobSourceMode, ()> {
    match s.trim().to_lowercase().as_str() {
        "rpc" => Ok(JobSourceMode::Rpc),
        "api" => Ok(JobSourceMode::Api),
        _ => Err(()),
    }
}

/// Parse config from TOML string. Used for tests.
pub fn parse_config_toml(s: &str) -> Result<PoolConfig, crate::PoolError> {
    let config: PoolConfig =
        toml::from_str(s).map_err(|e| crate::PoolError::Config(e.to_string()))?;
    config.pool.payout_script_pubkey_bytes()?;
    Ok(config)
}

/// Load config from file or return defaults. Environment overrides are applied after TOML.
pub fn load_config(path: Option<&str>) -> Result<PoolConfig, crate::PoolError> {
    let path = path.unwrap_or("config.toml");
    let mut config = match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str(&s).map_err(|e| crate::PoolError::Config(e.to_string()))?,
        Err(_) => PoolConfig::default(),
    };
    apply_env_overrides(&mut config);
    config.pool.payout_script_pubkey_bytes()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_source_mode_parses_rpc() {
        let config = parse_config_toml(
            r#"
[daemon]
url = "http://127.0.0.1:8332"
job_source_mode = "rpc"
"#,
        )
        .unwrap();
        assert_eq!(config.daemon.job_source_mode, JobSourceMode::Rpc);
    }

    #[test]
    fn test_job_source_mode_parses_api() {
        let config = parse_config_toml(
            r#"
[daemon]
url = "http://127.0.0.1:8332"
job_source_mode = "api"
"#,
        )
        .unwrap();
        assert_eq!(config.daemon.job_source_mode, JobSourceMode::Api);
    }

    #[test]
    fn test_job_source_mode_defaults_to_rpc() {
        let config = parse_config_toml(
            r#"
[daemon]
url = "http://127.0.0.1:8332"
"#,
        )
        .unwrap();
        assert_eq!(config.daemon.job_source_mode, JobSourceMode::Rpc);
    }

    #[test]
    fn test_job_source_mode_invalid_fails() {
        let err = parse_config_toml(
            r#"
[daemon]
url = "http://127.0.0.1:8332"
job_source_mode = "invalid"
"#,
        )
        .unwrap_err();
        assert!(matches!(err, crate::PoolError::Config(_)));
    }

    #[test]
    fn test_pool_payout_script_pubkey_empty_returns_none() {
        let config = parse_config_toml(
            r#"
[pool]
name = "azcoin-pool"
payout_script_pubkey_hex = ""
"#,
        )
        .unwrap();

        assert_eq!(config.pool.payout_script_pubkey_bytes().unwrap(), None);
    }

    #[test]
    fn test_pool_initial_difficulty_defaults_to_one() {
        let config = parse_config_toml(
            r#"
[pool]
name = "azcoin-pool"
"#,
        )
        .unwrap();

        assert_eq!(config.pool.initial_difficulty, 1);
    }

    #[test]
    fn test_pool_initial_difficulty_parses_from_toml() {
        let config = parse_config_toml(
            r#"
[pool]
name = "azcoin-pool"
initial_difficulty = 32
"#,
        )
        .unwrap();

        assert_eq!(config.pool.initial_difficulty, 32);
    }

    #[test]
    fn test_pool_payout_script_pubkey_valid_hex_decodes() {
        let config = parse_config_toml(
            r#"
[pool]
name = "azcoin-pool"
payout_script_pubkey_hex = "76a91400112233445566778899aabbccddeeff0011223388ac"
"#,
        )
        .unwrap();

        assert_eq!(
            config.pool.payout_script_pubkey_bytes().unwrap(),
            Some(hex::decode("76a91400112233445566778899aabbccddeeff0011223388ac").unwrap())
        );
    }

    #[test]
    fn test_pool_payout_script_pubkey_invalid_hex_fails() {
        let err = parse_config_toml(
            r#"
[pool]
name = "azcoin-pool"
payout_script_pubkey_hex = "abc"
"#,
        )
        .unwrap_err();

        assert!(matches!(err, crate::PoolError::Config(_)));
        assert!(err
            .to_string()
            .contains("invalid pool.payout_script_pubkey_hex"));
    }

    #[test]
    fn test_env_override_job_source_mode_takes_precedence() {
        let mut config = parse_config_toml(
            r#"
[daemon]
url = "http://127.0.0.1:8332"
job_source_mode = "rpc"
"#,
        )
        .unwrap();
        assert_eq!(config.daemon.job_source_mode, JobSourceMode::Rpc);

        let env = |k: &str| -> Option<String> {
            if k == env_keys::DAEMON_JOB_SOURCE_MODE {
                Some("api".to_string())
            } else {
                None
            }
        };
        apply_env_overrides_from(&mut config, &env);
        assert_eq!(config.daemon.job_source_mode, JobSourceMode::Api);
    }

    #[test]
    fn test_env_override_daemon_url_takes_precedence() {
        let mut config = parse_config_toml(
            r#"
[daemon]
url = "http://127.0.0.1:8332"
"#,
        )
        .unwrap();
        let env = |k: &str| -> Option<String> {
            if k == env_keys::DAEMON_URL {
                Some("http://node.example.com:8332".to_string())
            } else {
                None
            }
        };
        apply_env_overrides_from(&mut config, &env);
        assert_eq!(config.daemon.url, "http://node.example.com:8332");
    }

    #[test]
    fn test_env_override_rpc_credentials_take_precedence() {
        let mut config = parse_config_toml(
            r#"
[daemon]
url = "http://127.0.0.1:8332"
"#,
        )
        .unwrap();
        let env = |k: &str| -> Option<String> {
            match k {
                x if x == env_keys::DAEMON_RPC_USER => Some("rpcuser".to_string()),
                x if x == env_keys::DAEMON_RPC_PASSWORD => Some("rpcpass".to_string()),
                _ => None,
            }
        };
        apply_env_overrides_from(&mut config, &env);
        assert_eq!(config.daemon.rpc_user, "rpcuser");
        assert_eq!(config.daemon.rpc_password, "rpcpass");
    }

    #[test]
    fn test_env_override_api_and_stratum_take_precedence() {
        let mut config = parse_config_toml(
            r#"
[api]
bind = "0.0.0.0"
port = 8080
[stratum]
bind = "0.0.0.0"
port = 3333
"#,
        )
        .unwrap();
        let env = |k: &str| -> Option<String> {
            match k {
                x if x == env_keys::API_BIND => Some("127.0.0.1".to_string()),
                x if x == env_keys::API_PORT => Some("9090".to_string()),
                x if x == env_keys::STRATUM_BIND => Some("127.0.0.1".to_string()),
                x if x == env_keys::STRATUM_PORT => Some("4444".to_string()),
                _ => None,
            }
        };
        apply_env_overrides_from(&mut config, &env);
        assert_eq!(config.api.bind, "127.0.0.1");
        assert_eq!(config.api.port, 9090);
        assert_eq!(config.stratum.bind, "127.0.0.1");
        assert_eq!(config.stratum.port, 4444);
    }

    #[test]
    fn test_env_override_invalid_job_source_mode_ignored() {
        let mut config = parse_config_toml(
            r#"
[daemon]
url = "http://127.0.0.1:8332"
job_source_mode = "rpc"
"#,
        )
        .unwrap();
        let env = |k: &str| -> Option<String> {
            if k == env_keys::DAEMON_JOB_SOURCE_MODE {
                Some("invalid".to_string())
            } else {
                None
            }
        };
        apply_env_overrides_from(&mut config, &env);
        assert_eq!(config.daemon.job_source_mode, JobSourceMode::Rpc);
    }

    #[test]
    #[serial_test::serial]
    fn test_load_config_env_overrides_take_precedence_over_toml() {
        let dir = std::env::temp_dir();
        let path = dir.join("azcoin_pool_config_env_test.toml");
        let toml = r#"
[daemon]
url = "http://127.0.0.1:8332"
job_source_mode = "rpc"
[api]
port = 8080
[stratum]
port = 3333
"#;
        std::fs::write(&path, toml).unwrap();
        std::env::set_var(env_keys::DAEMON_JOB_SOURCE_MODE, "api");
        std::env::set_var(env_keys::DAEMON_URL, "http://override.example.com:8332");
        std::env::set_var(env_keys::API_PORT, "9999");

        let config = load_config(path.to_str()).unwrap();

        std::env::remove_var(env_keys::DAEMON_JOB_SOURCE_MODE);
        std::env::remove_var(env_keys::DAEMON_URL);
        std::env::remove_var(env_keys::API_PORT);
        let _ = std::fs::remove_file(&path);

        assert_eq!(config.daemon.job_source_mode, JobSourceMode::Api);
        assert_eq!(config.daemon.url, "http://override.example.com:8332");
        assert_eq!(config.api.port, 9999);
    }

    #[test]
    fn test_env_override_invalid_port_ignored() {
        let mut config = parse_config_toml(
            r#"
[api]
port = 8080
"#,
        )
        .unwrap();
        let env = |k: &str| -> Option<String> {
            if k == env_keys::API_PORT {
                Some("not_a_number".to_string())
            } else {
                None
            }
        };
        apply_env_overrides_from(&mut config, &env);
        assert_eq!(config.api.port, 8080);
    }
}
