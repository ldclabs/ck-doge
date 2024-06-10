use candid::CandidType;
use ck_doge_types::canister::*;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct MintedUtxo {
    pub block_index: u64,
    pub minted_at: u64, // timestamp in milliseconds
    pub utxo: Utxo,
}
