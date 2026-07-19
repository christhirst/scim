use serde::Deserialize;
use config::{Config, ConfigError, File};

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub http_host: String,
    pub http_port: u16,
    pub grpc_host: String,
    pub grpc_port: u16,
    pub auth_token: String,
}

impl Settings {
    pub fn new(config_path: &str) -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(File::with_name(config_path))
            .add_source(config::Environment::with_prefix("SCIM"))
            .build()?;

        s.try_deserialize()
    }
}
