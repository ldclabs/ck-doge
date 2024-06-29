use dogecoin::{canister::RPCAgent, jsonrpc::DogecoinRPC};
use std::collections::BTreeMap;
use std::time::Duration;

use crate::{ecdsa, store, SECONDS};

pub const REFRESH_PROXY_TOKEN_INTERVAL: u64 = 60 * 60; // 60 minutes
const FETCH_BLOCK_AFTER: u64 = 20; // 20 seconds

pub async fn refresh_proxy_token() {
    let (ecdsa_key_name, rpc_agent) =
        store::state::with(|s| (s.ecdsa_key_name.clone(), s.rpc_agents.clone()));
    update_proxy_token(ecdsa_key_name, rpc_agent).await;
}

pub async fn update_proxy_token(ecdsa_key_name: String, mut rpc_agents: Vec<RPCAgent>) {
    let mut tokens: BTreeMap<String, String> = BTreeMap::new();
    for agent in rpc_agents.iter_mut() {
        if let Some(token) = tokens.get(&agent.name) {
            agent.proxy_token = Some(token.clone());
            continue;
        }

        let token = ecdsa::sign_proxy_token(
            &ecdsa_key_name,
            (ic_cdk::api::time() / SECONDS) + REFRESH_PROXY_TOKEN_INTERVAL + 120,
            &agent.name,
        )
        .await
        .expect("failed to sign proxy token");
        tokens.insert(agent.name.clone(), token.clone());
        agent.proxy_token = Some(token);
    }

    store::state::with_mut(|r| r.rpc_agents = rpc_agents);
}

enum FetchBlockError {
    ShouldWait(String),
    Reorg(String),
    Other(String),
}

pub async fn fetch_block() {
    store::syncing::with_mut(|s| s.status = 1);
    let res: Result<(), FetchBlockError> = async {
        let agent = store::state::get_agent();
        let (tip_height, tip_blockhash) = store::state::with(|s| (s.tip_height, s.tip_blockhash));
        let height = if *tip_blockhash == [0u8; 32] {
            0
        } else {
            tip_height + 1
        };

        let ts = ic_cdk::api::time() / SECONDS;
        let key = format!("blk-{height}-{ts}");
        let blockhash = DogecoinRPC::get_blockhash(&agent, key.clone(), height)
            .await
            .map_err(FetchBlockError::ShouldWait)?;
        let block = DogecoinRPC::get_block(&agent, blockhash.to_string(), &blockhash)
            .await
            .map_err(FetchBlockError::Other)?;
        store::append_block(height, blockhash, block).map_err(FetchBlockError::Reorg)?;

        for attester in store::state::get_attest_agents() {
            let hash = DogecoinRPC::get_blockhash(&attester, key.clone(), height)
                .await
                .map_err(FetchBlockError::Other)?;
            if hash != blockhash {
                return Err(FetchBlockError::Other(format!(
                    "attester {} returned different blockhash at {}: {:?}, expected {:?}",
                    attester.name, height, hash, blockhash
                )));
            }
        }
        Ok(())
    }
    .await;

    match res {
        Err(FetchBlockError::Other(err)) => {
            ic_cdk::println!("fetch_block error: {}", err);
            store::state::with_mut(|s| s.append_error(err));
            store::syncing::with_mut(|s| s.status = -1);
        }
        Err(FetchBlockError::Reorg(err)) => {
            ic_cdk::println!("fetch_block Reorg error: {}", err);
            store::state::with_mut(|s| s.append_error(err));

            store::clear_for_restart_confirm_utxos();
            store::syncing::with_mut(|s| {
                s.timer = Some(ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                    ic_cdk::spawn(fetch_block())
                }));
            });
        }
        Err(FetchBlockError::ShouldWait(err)) => {
            store::state::with_mut(|s| s.append_error(err));
            store::syncing::with_mut(|s| {
                s.timer = Some(ic_cdk_timers::set_timer(
                    Duration::from_secs(FETCH_BLOCK_AFTER),
                    || ic_cdk::spawn(fetch_block()),
                ));
            });
        }
        Ok(_) => {
            store::state::with_mut(|s| s.last_errors.clear());
            store::syncing::with_mut(|s| {
                s.timer = Some(ic_cdk_timers::set_timer(
                    Duration::from_secs(0),
                    process_block,
                ));
            });
        }
    }
}

fn process_block() {
    store::syncing::with_mut(|s| s.status = 2);
    match store::process_block() {
        Err(err) => {
            ic_cdk::println!("process_block error: {}", err);
            store::syncing::with_mut(|s| s.status = -2);
            store::state::with_mut(|s| s.append_error(err));
        }
        Ok(res) => {
            store::syncing::with_mut(|s| {
                s.timer = Some(if res {
                    ic_cdk_timers::set_timer(Duration::from_secs(0), confirm_utxos)
                } else {
                    ic_cdk_timers::set_timer(
                        Duration::from_secs(0),
                        || ic_cdk::spawn(fetch_block()),
                    )
                });
            });
        }
    }
}

fn confirm_utxos() {
    store::syncing::with_mut(|s| s.status = 3);
    match store::confirm_utxos() {
        Err(err) => {
            ic_cdk::println!("confirm_utxos error: {}", err);
            store::syncing::with_mut(|s| s.status = -3);
            store::state::with_mut(|s| s.append_error(err));
        }
        Ok(res) => {
            store::syncing::with_mut(|s| {
                s.timer = Some(if res {
                    ic_cdk_timers::set_timer(Duration::from_secs(0), process_block)
                } else {
                    ic_cdk_timers::set_timer(
                        Duration::from_secs(0),
                        || ic_cdk::spawn(fetch_block()),
                    )
                });
            });
        }
    }
}
