use actix_web::{HttpResponse, ResponseError};
use alloy::primitives::hex;
use serde_json::json;
use std::fmt;

#[derive(Debug)]
pub enum SimulateError {
    AddressParseError(String),
    Uint256ParseError(String),
    InvalidOperation(String),
    ChainNotFound(u64),
    HexDecodeError(String),
    AnyhowError(String),
    SimulationError(String),
}

impl fmt::Display for SimulateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SimulateError::AddressParseError(e) => write!(f, "Address parse error: {}", e),
            SimulateError::Uint256ParseError(e) => write!(f, "Uint256 parse error: {}", e),
            SimulateError::InvalidOperation(e) => write!(f, "Invalid operation: {}", e),
            SimulateError::ChainNotFound(chain_id) => write!(f, "Chain not found: {}", chain_id),
            SimulateError::HexDecodeError(e) => write!(f, "Hex decode error: {}", e),
            SimulateError::AnyhowError(e) => write!(f, "Anyhow error: {}", e),
            SimulateError::SimulationError(e) => write!(f, "Simulation error: {}", e),
        }
    }
}

impl ResponseError for SimulateError {
    fn error_response(&self) -> HttpResponse {
        let error_message = format!("{}", self);
        HttpResponse::BadRequest().json(json!({ "error": error_message }))
    }
}

// 实现各种转换
impl From<hex::FromHexError> for SimulateError {
    fn from(err: hex::FromHexError) -> Self {
        Self::HexDecodeError(err.to_string())
    }
}

impl From<String> for SimulateError {
    fn from(err: String) -> Self {
        Self::AnyhowError(err)
    }
}
