use axum::{
    extract::State,
    response::Json,
    routing::post,
    Router,
};
use tower_http::cors::CorsLayer;
use std::net::SocketAddr;
use std::sync::Arc;
use serde_json::Value;

use crate::{RpcRequest, RpcResponse, RpcErrorResponse, RpcError, Result};
use crate::methods::RpcHandler;

pub struct RpcServer {
    handler: Arc<RpcHandler>,
    addr: SocketAddr,
}

impl RpcServer {
    pub fn new(addr: SocketAddr, handler: Arc<RpcHandler>) -> Self {
        Self { handler, addr }
    }
    
    pub async fn run(self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let app = Router::new()
            .route("/", post(handle_rpc_request))
            .route("/health", axum::routing::get(health_check))
            .layer(CorsLayer::permissive())
            .with_state(self.handler);
        
        tracing::info!("JSON-RPC server listening on {}", self.addr);
        
        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        axum::serve(listener, app).await?;
        
        Ok(())
    }
}

async fn handle_rpc_request(
    State(handler): State<Arc<RpcHandler>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    // Handle both single requests and batches
    if request.is_array() {
        // Batch request
        let requests: Vec<RpcRequest> = match serde_json::from_value(request) {
            Ok(reqs) => reqs,
            Err(e) => {
                return Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    },
                    "id": null
                }));
            }
        };
        
        let mut responses = Vec::new();
        for req in requests {
            let response = process_single_request(&handler, req).await;
            if response.id.is_some() {
                responses.push(response);
            }
        }
        
        Json(serde_json::to_value(responses).unwrap_or(Value::Null))
    } else {
        // Single request
        let request: RpcRequest = match serde_json::from_value(request) {
            Ok(req) => req,
            Err(e) => {
                return Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    },
                    "id": null
                }));
            }
        };
        
        let response = process_single_request(&handler, request).await;
        Json(serde_json::to_value(response).unwrap_or(Value::Null))
    }
}

async fn process_single_request(
    handler: &Arc<RpcHandler>,
    request: RpcRequest,
) -> RpcResponse {
    let id = request.id.clone();
    
    match handler.handle_request(request).await {
        Ok(result) => RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        },
        Err(error) => RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(RpcErrorResponse {
                code: error.code(),
                message: error.to_string(),
                data: None,
            }),
            id,
        },
    }
}

async fn health_check() -> &'static str {
    "OK"
}

pub struct WebSocketServer {
    handler: Arc<RpcHandler>,
    addr: SocketAddr,
}

impl WebSocketServer {
    pub fn new(addr: SocketAddr, handler: Arc<RpcHandler>) -> Self {
        Self { handler, addr }
    }
    
    pub async fn run(self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // WebSocket implementation would go here
        // For now, just a placeholder
        tracing::info!("WebSocket server would listen on {}", self.addr);
        Ok(())
    }
}