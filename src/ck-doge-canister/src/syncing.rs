use ck_doge_types::jsonrpc::DogecoinRPC;
use std::time::Duration;

use crate::{ecdsa, store, SECONDS};

pub const REFRESH_PROXY_TOKEN_INTERVAL: u64 = 60 * 5; // 5 minute
const FETCH_BLOCK_AFTER: u64 = 20; // 20 seconds

pub async fn refresh_proxy_token() {
    let (ecdsa_key_name, mut rpc_agent) =
        store::state::with(|s| (s.ecdsa_key_name.clone(), s.rpc_agent.clone()));
    let token = ecdsa::sign_proxy_token(
        &ecdsa_key_name,
        (ic_cdk::api::time() / SECONDS) + REFRESH_PROXY_TOKEN_INTERVAL + 120,
        &rpc_agent.name,
    )
    .await
    .expect("failed to sign proxy token");

    rpc_agent.proxy_token = Some(token);
    store::state::with_mut(|r| {
        r.rpc_agent = rpc_agent;
    });
}

enum FetchBlockError {
    ShouldWait(String),
    Other(String),
}

pub async fn fetch_block() {
    store::state::with_mut(|s| s.last_errors.clear());
    store::syncing::with_mut(|s| s.status = 1);
    let res: Result<(), FetchBlockError> = async {
        let agent = store::state::get_agent();
        let (tip_height, tip_blockhash) = store::state::with(|s| (s.tip_height, s.tip_blockhash));
        let height = if tip_blockhash == [0u8; 32] {
            0
        } else {
            tip_height + 1
        };

        let blockhash = DogecoinRPC::get_blockhash(&agent, format!("blk-{height}"), height)
            .await
            .map_err(FetchBlockError::ShouldWait)?;
        let block = DogecoinRPC::get_block(&agent, blockhash.to_string(), &blockhash)
            .await
            .map_err(FetchBlockError::Other)?;
        store::append_block(height, blockhash, block).map_err(FetchBlockError::Other)?;
        Ok(())
    }
    .await;

    match res {
        Err(FetchBlockError::Other(err)) => {
            store::state::with_mut(|s| s.append_error(err.clone()));
            store::syncing::with_mut(|s| s.status = -1);
            ic_cdk::trap(&err);
        }
        Err(FetchBlockError::ShouldWait(err)) => {
            store::state::with_mut(|s| s.append_error(err.clone()));
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
            store::syncing::with_mut(|s| s.status = -2);
            store::state::with_mut(|s| s.append_error(err.clone()));
            ic_cdk::trap(&err);
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
            store::syncing::with_mut(|s| s.status = -3);
            store::state::with_mut(|s| s.append_error(err.clone()));
            ic_cdk::trap(&err);
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
