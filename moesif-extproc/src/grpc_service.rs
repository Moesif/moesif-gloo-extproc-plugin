use log::{error, info, trace};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{ Response, Status};

use futures_util::StreamExt;
use std::sync::Arc;

use crate::config::Config;
use crate::event::{header_list_to_map, Event, ResponseInfo};
use crate::root_context::EventRootContext;
use crate::utils::*;

use envoy_ext_proc_proto::envoy::service::ext_proc::v3;

pub struct MoesifGlooExtProcGrpcService {
    config: Arc<Config>, // Store the config in the service
    event_context: Arc<EventRootContext>,
}

impl MoesifGlooExtProcGrpcService {
    pub fn new(config: Config) -> Result<Self, String> {
        // Initialize EventRootContext with the loaded configuration
        // This will also start the background task to consume the event queue
        let root_context: EventRootContext = EventRootContext::new(config.clone());

        // Create the service instance
        let service = MoesifGlooExtProcGrpcService {
            config: Arc::new(config),
            event_context: Arc::new(root_context),
        };

        Ok(service)
    }
}

#[tonic::async_trait]
impl v3::external_processor_server::ExternalProcessor for MoesifGlooExtProcGrpcService {
    type ProcessStream = ReceiverStream<Result<v3::ProcessingResponse, Status>>;

    async fn process(
        &self,
        request: tonic::Request<tonic::Streaming<v3::ProcessingRequest>>,
    ) -> Result<tonic::Response<Self::ProcessStream>, tonic::Status> {
        let mut stream: tonic::Streaming<v3::ProcessingRequest> = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(self.config.env.grpc_processing_queue_size);
        trace!("process called");

        let event_context = self.event_context.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut event = Event::new();
            let mut request_body_bytes = Vec::new();
            let mut response_body_bytes = Vec::new();

            while let Some(request) = stream.next().await {
                match request {
                    Ok(req) => {
                        // Process the ProcessingRequest and update the event
                        let response: v3::ProcessingResponse = process_request(
                            req,
                            &mut event,
                            &mut request_body_bytes,
                            &mut response_body_bytes,
                        );
                        // Send the ProcessingResponse back to the gateway
                        if let Err(e) = tx.send(Ok(response)).await {
                            trace!("Client closed connection: {:?}", e);
                        }
                    }
                    Err(e) => {
                        error!("Stream error: {:?}", e);
                        if let Err(e) = tx.send(Err(Status::internal("Internal error"))).await {
                            trace!("Client closed connection: {:?}", e);
                        }
                    }
                }
            }

            // After the stream ends, set user and company IDs and send the event
            event.set_user_and_company_ids(&config);
            event_context.push_event(event).await;
        });

        // Return the receiver stream to send replies to the gateway
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

// process the incoming processing request
fn process_request(
    request: v3::ProcessingRequest,
    event: &mut Event,
    request_body_bytes: &mut Vec<u8>,
    response_body_bytes: &mut Vec<u8>,
) -> v3::ProcessingResponse {
    let mut response = v3::ProcessingResponse::default();

    if let Some(req) = request.request {
        match req {
            v3::processing_request::Request::RequestHeaders(headers_msg) => {
                process_request_headers(&headers_msg, event);
                response.response = Some(v3::processing_response::Response::RequestHeaders(
                    v3::HeadersResponse::default(),
                ));
                trace!("Processed Request Headers");
            }
            v3::processing_request::Request::RequestBody(body_msg) => {
                process_request_body(&body_msg, event, request_body_bytes);
                response.response = Some(v3::processing_response::Response::RequestBody(
                    v3::BodyResponse::default(),
                ));
                trace!("Processed Request Body");
            }
            v3::processing_request::Request::RequestTrailers(_) => {
                response.response = Some(v3::processing_response::Response::RequestTrailers(
                    v3::TrailersResponse::default(),
                ));
                trace!("Processed Request Trailers");
            }
            v3::processing_request::Request::ResponseHeaders(headers_msg) => {
                process_response_headers(&headers_msg, event);
                response.response = Some(v3::processing_response::Response::ResponseHeaders(
                    v3::HeadersResponse::default(),
                ));
                trace!("Processed Response Headers");
            }
            v3::processing_request::Request::ResponseBody(body_msg) => {
                process_response_body(&body_msg, event, response_body_bytes);
                response.response = Some(v3::processing_response::Response::ResponseBody(
                    v3::BodyResponse::default(),
                ));
                trace!("Processed Response Body");
            }
            v3::processing_request::Request::ResponseTrailers(_) => {
                response.response = Some(v3::processing_response::Response::ResponseTrailers(
                    v3::TrailersResponse::default(),
                ));
                trace!("Processed Response Trailers");
            }
        }
    }

    response
}

fn process_request_headers(headers_msg: &v3::HttpHeaders, event: &mut Event) {
    let headers_map = header_list_to_map(headers_msg.headers.clone());
    event.request.set_headers(headers_map);
}

fn process_request_body(
    body_msg: &v3::HttpBody,
    event: &mut Event,
    request_body_bytes: &mut Vec<u8>,
) {
    request_body_bytes.extend_from_slice(&body_msg.body);
    if body_msg.end_of_stream {
        event.request.set_body(&request_body_bytes);
    }
}

fn process_response_headers(headers_msg: &v3::HttpHeaders, event: &mut Event) {
    if event.response.is_none() {
        event.response = Some(ResponseInfo::new());
    }
    if let Some(ref mut response_info) = event.response {
        let headers_map = header_list_to_map(headers_msg.headers.clone());
        response_info.set_headers(headers_map);
    }
}

fn process_response_body(
    body_msg: &v3::HttpBody,
    event: &mut Event,
    response_body_bytes: &mut Vec<u8>,
) {
    response_body_bytes.extend_from_slice(&body_msg.body);
    if body_msg.end_of_stream {
        if event.response.is_none() {
            event.response = Some(ResponseInfo::new());
        }
        if let Some(ref mut response_info) = event.response {
            response_info.set_body(&response_body_bytes);
        }
    }
}
