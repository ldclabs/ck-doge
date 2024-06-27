use std::time::Duration;

use crate::store;

pub const FINALIZE_BURNING_INTERVAL: u64 = 30; // seconds

pub async fn finalize_burning() {
    match store::finalize_burning().await {
        Ok(has_more) => {
            if has_more {
                ic_cdk::println!("finalize_burning: has more");
                ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                    ic_cdk::spawn(finalize_burning())
                });
            } else {
                ic_cdk::println!("finalize_burning: done");
            }
        }
        Err(err) => {
            ic_cdk::println!("finalize_burning error: {}", err);
        }
    }
}

pub const CLEAR_UTXOS_INTERVAL: u64 = 600; //seconds

pub async fn collect_and_clear_utxos() {
    match store::collect_and_clear_utxos().await {
        Ok(_) => {
            ic_cdk::println!("collect_and_clear_utxos: done");
        }
        Err(err) => {
            ic_cdk::println!("collect_and_clear_utxos error: {}", err);
        }
    }
}
