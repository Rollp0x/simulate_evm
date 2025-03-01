use actix_web::{web, App, HttpServer};
use alloy::{
    providers::{ProviderBuilder, RootProvider},
    transports::http::{Client, Http},
};
use futures::FutureExt;
use revm_trace::{
    create_evm_with_inspector,
    types::{TokenInfo, NATIVE_TOKEN_ADDRESS},
    utils::erc20_utils::get_token_infos,
    TransactionProcessor, TxInspector,
};
use std::{collections::HashMap, env, fs};
use tokio::sync::mpsc;

mod error;
mod trace;
mod types;
use types::{ChainInfo, TraceResponse, TxRequest};

#[derive(Clone)]
pub struct AppState {
    pub chains: HashMap<u64, RootProvider<Http<Client>>>,
    pub trace_tx: mpsc::Sender<TxRequest>,
}

// 修改服务器由actix::main到tokio::main
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let chains = fs::read_to_string("./config/chains.json").expect("Unable to read chains.json");
    let chains: Vec<ChainInfo> =
        serde_json::from_str(&chains).expect("Unable to parse chains.json");
    // 这里vec转hashmap
    let mut chains_map = HashMap::new();
    for chain in chains {
        chains_map.insert(chain.chain_id, chain);
    }
    let mut supported_chains = HashMap::new();
    for chain in chains_map.values() {
        let provider =
            ProviderBuilder::new().on_http(chain.rpc_url.parse().expect("Invalid RPC URL"));
        supported_chains.insert(chain.chain_id, provider);
    }

    // 创建 EVM 实例
    println!("Creating EVM instances...");
    let mut evm_cache = HashMap::new();
    for chain in chains_map.values() {
        let evm = create_evm_with_inspector(&chain.rpc_url, TxInspector::new())
            .await
            .unwrap_or_else(|e| {
                eprintln!(
                    "Failed to create EVM instance for chain {}: {}",
                    chain.chain_id, e
                );
                std::process::exit(1);
            });
        evm_cache.insert(chain.chain_id, evm);
    }

    // 创建通信通道
    let (tx, mut rx) = mpsc::channel::<TxRequest>(32);
    // 创建app state
    let app_state = web::Data::new(AppState {
        chains: supported_chains,
        trace_tx: tx,
    });
    // 创建 HTTP 服务器
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let server = HttpServer::new(move || App::new().app_data(app_state.clone()))
        .bind(format!("127.0.0.1:{}", port))?
        .run();
    // 启动服务器但不等待
    let server_handle = server.handle();
    tokio::spawn(server);
    println!("✅ Configuration validated successfully");
    println!("🚀 Server starting at port:{}", port);
    // 处理请求的任务 这里不考虑并发，不能使用unwrap
    while let Some(req) = rx.recv().await {
        // 在每次循环开始时检查 Ctrl+C 信号
        if tokio::signal::ctrl_c().now_or_never().is_some() {
            println!("\n🛑 Received Ctrl+C, shutting down...");
            break;
        }
        // req不可能为空
        let TxRequest {
            chain_id,
            txs,
            response_tx,
        } = req;
        // 这里不可能unwrap，因为如果 chain_id不对前面的provider那返回了
        // let Some是新语法
        let Some(evm) = evm_cache.get_mut(&chain_id) else {
            let _ = response_tx.send(TraceResponse {
                result: Err(format!("EVM instance not found for chain {}", chain_id)),
                token_infos: None,
            });
            continue;
        };
        let result = evm.process_transactions(txs);
        // 统计所有代币地址
        let mut tokens = Vec::new();
        for (trace_result, trace_output) in result.iter().flatten() {
            if trace_result.is_success() {
                for transfer in &trace_output.asset_transfers {
                    let token = transfer.token;
                    if token != NATIVE_TOKEN_ADDRESS && !tokens.contains(&token) {
                        tokens.push(token);
                    }
                }
            }
        }
        // 获取代币信息
        let mut token_infos_map = HashMap::new();
        let token_infos = get_token_infos(evm, &tokens, None);
        if let Ok(token_infos) = token_infos {
            for (index, info) in token_infos.into_iter().enumerate() {
                let token = tokens[index];
                token_infos_map.insert(token, info);
            }
        } else {
            let _ = response_tx.send(TraceResponse {
                result: Err(format!(
                    "Failed to get token infos: {}",
                    token_infos.err().unwrap()
                )),
                token_infos: None,
            });
            continue;
        }
        let native_info = chains_map.get(&chain_id).unwrap();
        // 添加原生代币信息
        token_infos_map.insert(
            NATIVE_TOKEN_ADDRESS,
            TokenInfo {
                symbol: native_info.symbol.clone(),
                decimals: native_info.decimals,
            },
        );

        // 发送响应
        let _ = response_tx.send(TraceResponse {
            result: Ok(result),
            token_infos: Some(token_infos_map),
        });
    }
    // 优雅关闭
    println!("Stopping HTTP server...");
    server_handle.stop(true).await;
    println!("👋 Shutdown complete");
    Ok(())
}
