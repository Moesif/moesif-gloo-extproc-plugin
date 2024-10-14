use crate::config::Config;
use crate::event::Event;
use reqwest::header::HeaderMap as ReqwestHeaderMap;

use bytes::Bytes;
use log::LevelFilter;

type Headers = Vec<(String, String)>;


pub fn generate_curl_command(
    method: &str,
    url: &str,
    headers: &ReqwestHeaderMap,
    body: Option<&Bytes>,
) -> String {
    let mut curl_cmd = format!("curl -v -X {} '{}'", method, url);

    // Add headers to the curl command
    for (key, value) in headers {
        let header_value = value.to_str().unwrap_or("");
        curl_cmd.push_str(&format!(" -H '{}: {}'", key, header_value));
    }

    // Add body to the curl command
    if let Some(body) = body {
        let body_str = std::str::from_utf8(body).unwrap_or("");
        curl_cmd.push_str(&format!(" --data '{}'", body_str));
    }

    curl_cmd
}

pub fn get_header(headers: &Headers, name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, header_value)| header_value.to_owned())
}

pub fn set_and_display_log_level(config: &Config) {
    // Check if RUST_LOG is set
    if let Some(rust_log) = &config.env.rust_log {
        match rust_log.to_lowercase().as_str() {
            "trace" => log::set_max_level(LevelFilter::Trace),
            "debug" => log::set_max_level(LevelFilter::Debug),
            "info" => log::set_max_level(LevelFilter::Info),
            "warn" => log::set_max_level(LevelFilter::Warn),
            "error" => log::set_max_level(LevelFilter::Error),
            _ => {
                // If RUST_LOG is set to an invalid value, fall back to default logic
                set_level_based_on_debug(config);
            }
        }
    } else {
        // If RUST_LOG is not set, use the DEBUG environment variable logic
        set_level_based_on_debug(config);
    }

    log::info!("Configuration: {:?}", config);

    // Display the current log level
    match log::max_level() {
        LevelFilter::Error => println!("Logging level set to: ERROR"),
        LevelFilter::Warn => println!("Logging level set to: WARN"),
        LevelFilter::Info => println!("Logging level set to: INFO"),
        LevelFilter::Debug => println!("Logging level set to: DEBUG"),
        LevelFilter::Trace => println!("Logging level set to: TRACE"),
        LevelFilter::Off => println!("Logging is turned OFF"),
    }
}

fn set_level_based_on_debug(config: &Config) {
    if config.env.debug {
        log::set_max_level(LevelFilter::Trace);
    } else {
        log::set_max_level(LevelFilter::Warn);
    }
}
