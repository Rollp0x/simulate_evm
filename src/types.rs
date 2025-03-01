use std::collections::HashMap;
use alloy::primitives::Address;
use serde::{Serialize, Deserialize};
use revm_trace::{
    errors::EvmError, 
    inspectors::TxTraceOutput, 
    types::{ExecutionResult, TokenInfo}, 
    SimulationBatch, 
};

type TraceResult = Result<Vec<Result<(ExecutionResult, TxTraceOutput),EvmError>>, String>;

/// 处理模拟交易返回的结果
pub struct TraceResponse {
    pub result: TraceResult,
    pub token_infos:Option<HashMap<Address,TokenInfo>>, // 模拟交易返回的token信息
}

/// 通道发过来的准备模拟交易的请求
pub struct TxRequest {
    pub chain_id:u64,
    pub txs:SimulationBatch,
    pub response_tx:tokio::sync::oneshot::Sender<TraceResponse>,
}

/// 模拟交易返回的结果
#[derive(Serialize,Debug,Clone,Default)]
pub struct SimulationResult {
    pub block_number:u64,   // 模拟交易的区块高度
    pub error:Option<String>, // 模拟交易返回的错误
    pub execution_result:Option<ExecutionResult>, // 模拟交易执行的结果
    pub trace_result:Option<TxTraceOutput>, // 模拟交易返回的结果
}

impl SimulationResult {
    pub fn from_block_number(block_number:u64) -> Self {
        SimulationResult {
            block_number,
            error:None,
            execution_result:None,
            trace_result:None,
        }
    }
}

#[derive(Serialize,Debug,Clone,Default)]
pub struct BatchSimulationResult {
    pub results:Vec<SimulationResult>,
    pub token_infos:Option<HashMap<Address,TokenInfo>>, // 模拟交易返回的token信息
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainInfo {
    // pub name: String,
    pub chain_id: u64,
    pub rpc_url: String,
    pub symbol: String,
    pub decimals: u8,
    // #[serde(default)]
    // pub multisend_address: Option<String>,
    // pub wrap_token: String,         // 新增
    // pub wrap_token_symbol: String,  // 新增
}