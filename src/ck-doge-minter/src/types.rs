use candid::CandidType;
use dogecoin::canister::*;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct MintedUtxo {
    pub block_index: u64,
    pub minted_at: u64, // timestamp in milliseconds
    pub utxo: Utxo,
}

pub type MintMemo = OutPoint;
#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct BurnMemo {
    pub address: Address,
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct MintOutput {
    pub amount: u64,
    pub instructions: u64,
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct BurnInput {
    pub address: String,
    pub amount: u64,
    pub fee_rate: u64, // units per vByte, should >= 1000
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct BurnOutput {
    pub txid: Txid,
    pub tip_height: u64,
    pub instructions: u64,
}
