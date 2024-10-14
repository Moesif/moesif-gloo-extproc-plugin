use std::collections::HashMap;
use base64::Engine;
use envoy_ext_proc_proto::envoy::config::core::v3::HeaderMap;
use log::{info, trace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use chrono::Utc;


use std::net::IpAddr;
use std::str::FromStr;

use crate::config::Config;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct RequestInfo {
    pub time: String,
    pub verb: String,
    pub uri: String,
    pub headers: HashMap<String, String>,
    pub transfer_encoding: Option<String>,
    pub api_version: Option<String>,
    pub ip_address: Option<String>,
    pub body: Value,
}

impl RequestInfo {
    pub fn new() -> Self {
        Self {
            time: Utc::now().to_rfc3339(),
            ..Default::default()
        }
    }

    pub fn set_headers(&mut self, headers: HashMap<String, String>) {
        self.headers = headers;

        // Extract verb and URI from headers
        if let Some(method) = self.headers.get(":method") {
            self.verb = method.clone();
        }
        if let Some(path) = self.headers.get(":path") {
            self.uri = path.clone();
        }

        // Remove pseudo-headers
        self.headers.retain(|k, _| !k.starts_with(":"));

        // Extract other fields
        self.api_version = self.headers.get("x-api-version").cloned();
        self.ip_address = get_client_ip(&self.headers);
    }

    pub fn set_body(&mut self, body_bytes: &[u8]) {
        if !body_bytes.is_empty() {
            (self.body, self.transfer_encoding) = encode_body(body_bytes);
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct ResponseInfo {
    pub time: String,
    pub status: usize,
    pub headers: HashMap<String, String>,
    pub ip_address: Option<String>,
    pub body: Value,
    pub transfer_encoding: Option<String>, // Ensure this is included
}


impl ResponseInfo {
    pub fn new() -> Self {
        Self {
            time: Utc::now().to_rfc3339(),
            ..Default::default()
        }
    }

    pub fn set_headers(&mut self, headers: HashMap<String, String>) {
        self.headers = headers;

        // Extract status code
        if let Some(status) = self.headers.get(":status") {
            if let Ok(status_code) = status.parse::<usize>() {
                self.status = status_code;
            }
        }

        // Remove pseudo-headers
        self.headers.retain(|k, _| !k.starts_with(":"));
    }

    pub fn set_body(&mut self, body_bytes: &[u8]) {
        if !body_bytes.is_empty() {
            (self.body, self.transfer_encoding) = encode_body(body_bytes);
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Event {
    pub request: RequestInfo,
    pub response: Option<ResponseInfo>,
    pub user_id: Option<String>,
    pub company_id: Option<String>,
    pub metadata: Value,
    pub direction: String,
    pub session_token: Option<String>,
    pub blocked_by: Option<String>,
}

impl Event {
    pub fn new() -> Self {
        Self {
            request: RequestInfo::new(),
            direction: "Incoming".to_string(),
            ..Default::default()
        }
    }

    pub fn set_user_and_company_ids(&mut self, config: &Config) {
        if let Some(user_id_header) = &config.env.user_id_header {
            if let Some(user_id) = self.request.headers.get(user_id_header) {
                trace!("Setting user_id: {}", user_id);
                self.user_id = Some(user_id.clone());
            }
        }
        if let Some(company_id_header) = &config.env.company_id_header {
            if let Some(company_id) = self.request.headers.get(company_id_header) {
                trace!("Setting company_id: {}", company_id);
                self.company_id = Some(company_id.clone());
            }
        }
    }
}

pub fn get_client_ip(headers: &HashMap<String, String>) -> Option<String> {
    let possible_headers = vec![
        "x-client-ip",
        "x-forwarded-for",
        "cf-connecting-ip",
        "fastly-client-ip",
        "true-client-ip",
        "x-real-ip",
        "x-cluster-client-ip",
        "x-forwarded",
        "forwarded-for",
        "forwarded",
        "x-appengine-user-ip",
        "cf-pseudo-ipv4",
    ];

    for header in possible_headers {
        if let Some(value) = headers.get(header) {
            let ips: Vec<&str> = value.split(',').collect();
            for ip in ips {
                if IpAddr::from_str(ip.trim()).is_ok() {
                    return Some(ip.trim().to_string());
                }
            }
        }
    }
    None
}

fn encode_body(body_bytes: &[u8]) -> (Value, Option<String>) {
    match serde_json::from_slice::<Value>(body_bytes) {
        Ok(json_value) => {
            (json_value, None)
        }
        Err(_) => {
            let encoded_body = base64::engine::general_purpose::STANDARD.encode(body_bytes);
            let body = Value::String(encoded_body);
            (body, Some("base64".to_string()))
        }
    }
}

pub fn header_list_to_map(header_map: Option<HeaderMap>) -> HashMap<String, String> {
    let mut map = HashMap::new();

    if let Some(header_map) = header_map {
        for header in header_map.headers {
            let key = header.key.to_lowercase();
            let value = if header.value.is_empty() {
                String::from_utf8_lossy(&header.raw_value).to_string()
            } else {
                header.value
            };
            if let Some(existing_value) = map.get(&key) {
                map.insert(key, format!("{}, {}", existing_value, value));
            } else {
                map.insert(key, value);
            }
        }
    }

    map
}