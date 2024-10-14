use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env};

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub env: EnvConfig,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct EnvConfig {
    pub moesif_application_id: String,
    // use serde to make these values to_lowercase
    pub user_id_header: Option<String>,
    pub company_id_header: Option<String>,
    #[serde(default = "default_batch_max_size")]
    pub batch_max_size: usize,
    #[serde(default = "default_batch_max_wait")]
    pub batch_max_wait: u64,
    #[serde(default = "default_queue_max_size")]
    pub queue_max_size: usize,
    #[serde(default = "default_grpc_processing_queue_size")]
    pub grpc_processing_queue_size: usize,
    #[serde(default = "default_base_uri")]
    pub base_uri: String,
    #[serde(default = "default_debug")]
    pub debug: bool,
    #[serde(default = "connection_timeout")]
    pub connection_timeout: u64,
    pub rust_log: Option<String>,
}

fn default_batch_max_size() -> usize {
    100
}

fn default_batch_max_wait() -> u64 {
    2000
}

fn default_queue_max_size() -> usize {
    10000
}

fn default_grpc_processing_queue_size() -> usize {
    4
}

fn default_base_uri() -> String {
    "https://api.moesif.net".to_string()
}

fn default_debug() -> bool {
    false
}

fn connection_timeout() -> u64 {
    5000
}

impl EnvConfig {
    pub fn new() -> Self {
        let mut env = match envy::from_env::<EnvConfig>() {
            Ok(env) => env,
            Err(_) => {
                log::error!("Failed to load environment variables, using defaults.");
                EnvConfig::default()
            }
        };
        env.post_process();
        if let Err(e) = env.validate() {
            log::error!("Invalid configuration: {}", e);
        }
        env
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.moesif_application_id.is_empty() {
            return Err("moesif_application_id cannot be empty.".to_string());
        }
        if self.batch_max_size == 0 {
            return Err("batch_max_size cannot be zero.".to_string());
        }
        if self.batch_max_wait == 0 {
            return Err("batch_max_wait cannot be zero.".to_string());
        }
        if self.queue_max_size == 0 {
            return Err("queue_max_size cannot be zero.".to_string());
        }
        if self.grpc_processing_queue_size == 0 {
            return Err("grpc_processing_queue_size cannot be zero.".to_string());
        }
        if self.connection_timeout == 0 {
            return Err("connection_timeout cannot be zero.".to_string());
        }
        if self.base_uri.is_empty() {
            return Err("base_uri cannot be empty.".to_string());
        }
        Ok(())
    }
    fn post_process(&mut self) {
        self.user_id_header = self.user_id_header.as_ref().map(|s| s.to_lowercase());
        self.company_id_header = self.company_id_header.as_ref().map(|s| s.to_lowercase());
    }
}


//TODO load dynamic from config api on update
#[derive(Default, Serialize, Deserialize, Debug)]
pub struct AppConfigResponse {
    pub org_id: String,
    pub app_id: String,
    pub sample_rate: i32,
    pub block_bot_traffic: bool,
    pub user_sample_rate: HashMap<String, i32>,
    pub company_sample_rate: HashMap<String, i32>,
    pub user_rules: HashMap<String, Vec<EntityRuleValues>>,
    pub company_rules: HashMap<String, Vec<EntityRuleValues>>,
    pub ip_addresses_blocked_by_name: HashMap<String, String>,
    pub regex_config: Vec<RegexRule>,
    pub billing_config_jsons: HashMap<String, String>,
    pub e_tag: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct EntityRuleValues {
    pub rules: String,
    pub values: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct RegexRule {
    pub conditions: Vec<RegexCondition>,
    pub sample_rate: i32,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct RegexCondition {
    pub path: String,
    pub value: String,
}