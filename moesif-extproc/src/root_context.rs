use std::time::Duration;

use crate::config::Config;
use crate::utils::*;
use log::{info, trace};
use reqwest::header::{HeaderMap as ReqwestHeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};

use crate::event::Event;
use bytes::Bytes;
use tokio::sync::mpsc;

type CallbackType = Box<dyn Fn(Vec<(String, String)>, Option<Vec<u8>>) + Send>;

#[derive(Clone)]
pub struct EventRootContext {
    pub config: Config,
    pub event_sender: mpsc::Sender<Bytes>,
    pub client: Client,
}

impl EventRootContext {
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.env.connection_timeout as u64))
            .build()
            .expect("Failed to build HTTP client");

        let (event_sender, event_receiver) = mpsc::channel::<Bytes>(config.env.queue_max_size);

        let root_context = EventRootContext {
            config: config.clone(),
            event_sender: event_sender,
            client: client.clone(),
        };

        let cloned_context = root_context.clone();
        // Start background task to process events
        tokio::spawn(async move {
            cloned_context.run_event_processor(event_receiver).await;
        });

        root_context
    }

    pub async fn push_event(&self, event: Event) {
        match serde_json::to_vec(&event) {
            Ok(event_bytes) => {
                // Send event to the channel, await if queue is full
                if let Err(e) = self.event_sender.send(Bytes::from(event_bytes)).await {
                    log::error!("Failed to send event to queue: {:?}", e);
                } else {
                    log::trace!("Event sent to queue: {:?}", event);
                }
            },
            Err(e) => {
                log::error!("Failed to serialize event: {:?}", e);
            }
        }
    }


    async fn run_event_processor(&self, mut event_receiver: mpsc::Receiver<Bytes>) {
        let mut batcher = Batcher::new(
            self.config.env.batch_max_size,
            self.config.env.batch_max_wait,
        );

        loop {
            tokio::select! {
                Some(event) = event_receiver.recv() => {
                    batcher.handle_new_event(event).await;
                    if batcher.should_flush() {
                        self.flush_buffer(&mut batcher).await;
                    }
                },
                _ = tokio::time::sleep(batcher.calculate_timeout()), if batcher.has_events() => {
                    self.flush_buffer(&mut batcher).await;
                },
            }
        }
    }

    async fn flush_buffer(&self, batcher: &mut Batcher) {
        self.send_batch(&batcher.buffer).await;
        batcher.reset();
    }

    async fn send_batch(&self, buffer: &Vec<Bytes>) {
        if buffer.is_empty() {
            return;
        }

        let body = self.write_events_json(buffer).await;
        info!("Posting {} events.", buffer.len());

        if let Err(e) = self
            .dispatch_http_request(
                "POST",
                "/v1/events/batch",
                body,
                Box::new(|headers, _| {
                    let config_etag = get_header(&headers, "X-Moesif-Config-Etag");
                    let rules_etag = get_header(&headers, "X-Moesif-Rules-Etag");
                    trace!(
                        "Event Response eTags: config={:?} rules={:?}",
                        config_etag,
                        rules_etag
                    );
                }),
            )
            .await
        {
            log::error!("Failed to dispatch HTTP request: {:?}", e);
        }
    }

    async fn write_events_json(&self, events: &Vec<Bytes>) -> Bytes {
        log::trace!("Entering write_events_json with {} events.", events.len());

        // Calculate the total size needed for all event bytes
        let total_events_size: usize = events.iter().map(|event_bytes| event_bytes.len()).sum();

        // Each comma between events adds 1 byte, '[' and ']' add 2 bytes
        let num_commas = if events.len() > 1 {
            events.len() - 1
        } else {
            0
        };
        let json_array_size = total_events_size + num_commas + 2;

        let mut event_json_array = Vec::with_capacity(json_array_size);

        event_json_array.push(b'[');

        for (i, event_bytes) in events.iter().enumerate() {
            if i > 0 {
                event_json_array.push(b',');
            }
            event_json_array.extend_from_slice(event_bytes);

            log::trace!(
                "Adding event to JSON array: {}",
                String::from_utf8_lossy(event_bytes)
            );
        }

        event_json_array.push(b']');

        log::trace!(
            "Final JSON array being sent, length {}: {}",
            event_json_array.len(),
            String::from_utf8_lossy(&event_json_array)
        );
        event_json_array.into() // Return as Bytes
    }

    async fn dispatch_http_request(
        &self,
        method: &str,
        path: &str,
        body: Bytes,
        callback: CallbackType,
    ) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        log::trace!("Entering dispatch_http_request.");

        let url = format!("{}{}", self.config.env.base_uri, path);

        let method = Method::from_bytes(method.as_bytes())?;
        log::trace!("Using method: {} and URL: {}", method, url);

        let mut headers = ReqwestHeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("x-moesif-application-id"),
            HeaderValue::from_str(&self.config.env.moesif_application_id)?,
        );

        let curl_cmd = generate_curl_command(method.as_str(), &url, &headers, Some(&body));
        log::trace!("Equivalent curl command:\n{}", curl_cmd);

        log::trace!(
            "Dispatching {} request to {} with headers: {:?} and body: {}",
            method,
            url,
            headers,
            std::str::from_utf8(&body).unwrap_or_default()
        );

        let response = self
            .client
            .request(method, &url)
            .headers(headers)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        log::trace!("Received response with status: {}", status);

        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();

        let body = response.bytes().await.ok();

        // Call the provided callback with the headers and response body
        callback(headers, body.map(|b| b.to_vec()));

        log::trace!("Exiting dispatch_http_request.");

        Ok(status.as_u16().into())
    }
}

struct Batcher {
    buffer: Vec<Bytes>,
    first_event_time: Option<tokio::time::Instant>,
    max_size: usize,
    max_wait: u64,
}

impl Batcher {
    fn new(max_size: usize, max_wait: u64) -> Self {
        Batcher {
            buffer: Vec::new(),
            first_event_time: None,
            max_size,
            max_wait,
        }
    }

    fn calculate_timeout(&self) -> Duration {
        if let Some(time) = self.first_event_time {
            let elapsed = time.elapsed();
            if elapsed >= Duration::from_millis(self.max_wait) {
                Duration::from_millis(0)
            } else {
                Duration::from_millis(self.max_wait) - elapsed
            }
        } else {
            Duration::from_secs(u64::MAX)
        }
    }

    async fn handle_new_event(&mut self, event: Bytes) {
        if self.first_event_time.is_none() {
            self.first_event_time = Some(tokio::time::Instant::now());
        }
        self.buffer.push(event);
    }

    fn should_flush(&self) -> bool {
        self.buffer.len() >= self.max_size
    }

    fn has_events(&self) -> bool {
        self.first_event_time.is_some()
    }

    fn reset(&mut self) {
        self.first_event_time = None;
        self.buffer.clear();
    }
}
