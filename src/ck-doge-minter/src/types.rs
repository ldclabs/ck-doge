use candid::{CandidType, Principal};
use dogecoin::canister::*;
use serde::{Deserialize, Serialize};

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct MintedUtxo {
    pub block_index: u64,
    pub minted_at: u64, // timestamp in milliseconds
    pub utxo: Utxo,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct CollectedUtxo {
    pub principal: Principal,
    pub block_index: u64,
    pub height: u64,
    pub utxo: Utxo,
}

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct BurnedUtxos {
    pub block_index: u64,
    pub txid: Txid,
    pub height: u64,
    pub address: Address,
    pub utxos: Vec<Utxo>,
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
    pub block_index: u64,
    pub txid: Txid,
    pub tip_height: u64,
    pub instructions: u64,
}
