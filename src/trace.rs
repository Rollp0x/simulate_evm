use crate::{
    error::SimulateError,
    types::{BatchSimulationResult, SimulationResult, TraceResponse, TxRequest},
    AppState
};
use actix_web::{post, web, HttpResponse};
use alloy::{
    eips::BlockNumberOrTag,
    primitives::{hex, Address, TxKind, U256},
    providers::Provider,
    transports::Transport,
};
use revm_trace::{BlockEnv, SimulationBatch, SimulationTx};
use serde::Deserialize;
use serde_json::json;
use tokio::{
    sync::oneshot,
    time::{timeout, Duration},
};

#[derive(Deserialize)]
pub struct TraceRequestSingle {
    pub from: String,
    pub to: Option<String>, // 部署合约就没有to
    pub value: Option<String>,
    pub data: Option<String>,
    pub operation: u8, // 0 => call ,1是delegate call , 2 是create
}

#[derive(Deserialize)]
pub struct BatchTraceRequest {
    pub chain_id: u64,                     // 链id
    pub is_stateful: bool,                 // 是否是stateful
    pub block_number: Option<u64>,         // 指定的模拟交易区块高度
    pub requests: Vec<TraceRequestSingle>, // 请求体
}

/// 批量模拟交易
#[post("/simulate/batch")]
async fn simulate_batch(
    req: web::Json<BatchTraceRequest>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, SimulateError> {
    // 1. 解析请求
    let BatchTraceRequest {
        chain_id,
        is_stateful,
        block_number,
        requests,
    } = req.into_inner();
    let provider = state
        .chains
        .get(&chain_id)
        .ok_or(SimulateError::ChainNotFound(chain_id))?;
    // 2. 构造原始交易
    let mut transactions = vec![];
    for request in requests {
        let from_address = parse_address(&request.from)?;
        let to_address = if request.operation == 2 {
            None
        } else if request.operation == 0 {
            let to_address = request.to.map(|v| parse_address(&v)).transpose()?;
            if to_address.is_none() {
                return Err(SimulateError::InvalidOperation(
                    "Operation call must have a to address".to_string(),
                ));
            } else {
                to_address
            }
        } else {
            return Err(SimulateError::InvalidOperation(
                "Invalid operation type".to_string(),
            ));
        };
        let value = request
            .value
            .map(|v| parse_u256(&v))
            .transpose()?
            .unwrap_or(U256::ZERO);
        let data = request
            .data
            .map(hex::decode)
            .transpose()?
            .unwrap_or_default();
        transactions.push(SimulationTx {
            caller: from_address,
            transact_to: if let Some(to) = to_address {
                TxKind::Call(to)
            } else {
                TxKind::Create
            },
            value,
            data: data.into(),
        });
    }
    let block_env = get_block_env(block_number, provider).await?;
    let block_number = block_env.number; // replace block_number
    let batch = SimulationBatch {
        block_env,
        transactions,
        is_stateful,
    };
    // 发送到main线程进行处理
    let (response_tx, response_rx) = oneshot::channel();
    state
        .trace_tx
        .send(TxRequest {
            chain_id,
            txs: batch,
            response_tx,
        })
        .await
        .map_err(|e| {
            SimulateError::SimulationError(format!("Failed to send trace request: {}", e))
        })?;
    // 设置超时时间
    const TIMEOUT_DURATION: Duration = Duration::from_secs(300);
    let response: TraceResponse = timeout(TIMEOUT_DURATION, response_rx)
        .await
        .map_err(|_| SimulateError::SimulationError("Trace request timed out".to_string()))?
        .map_err(|_| SimulateError::SimulationError("Response channel closed".to_string()))?;
    // 处理返回的模拟交易
    if let Ok(results) = response.result {
        let mut final_results = vec![];
        // 遍历每个模拟交易的结果
        for r in results.into_iter() {
            let mut sim_result = SimulationResult::from_block_number(block_number);
            // 如果模拟成功，则将结果添加到final_results中
            if let Ok((trace_result, trace_output)) = r {
                sim_result.trace_result = Some(trace_output);
                sim_result.execution_result = Some(trace_result);
                sim_result.error = None;
            } else {
                // 如果模拟失败，则将错误信息添加到final_results中
                sim_result.error = Some(format!("Trace Error:{:?}", r.unwrap_err()));
            }
            final_results.push(sim_result);
        }
        // 这里只返回模拟结果
        Ok(HttpResponse::Ok().json(json!(BatchSimulationResult {
            results: final_results,
            token_infos: response.token_infos
        })))
    } else {
        Err(SimulateError::SimulationError(response.result.unwrap_err()))
    }
}

// 创建专门的解析函数来减少重复代码
fn parse_u256(value_str: &str) -> Result<U256, SimulateError> {
    U256::from_str_radix(value_str, 10).map_err(|_| {
        SimulateError::Uint256ParseError(format!(
            "Invalid uint256 radix of decimal format: {}",
            value_str
        ))
    })
}

fn parse_address(addr_str: &str) -> Result<Address, SimulateError> {
    addr_str.parse::<Address>().map_err(|_| {
        SimulateError::AddressParseError(format!("Invalid address format: {}", addr_str))
    })
}

async fn get_block_env<T, P>(block_number: Option<u64>, provider: &P) -> Result<BlockEnv, String>
where
    T: Transport + Clone,
    P: Provider<T>,
{
    let number = if let Some(number) = block_number {
        number
    } else {
        provider
            .get_block_number()
            .await
            .map_err(|e| e.to_string())?
    };
    let timestamp = provider
        .get_block_by_number(BlockNumberOrTag::Number(number), false)
        .await
        .map_err(|e| e.to_string())?
        .map(|block| block.header.timestamp)
        .ok_or("Block get failed".to_string())?;
    Ok(BlockEnv { number, timestamp })
}
