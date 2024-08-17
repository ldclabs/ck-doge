use bitcoin::hashes::{sha256d, Hash};
use candid::CandidType;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use sha3::{Digest, Sha3_256};

mod agent;

pub use agent::*;

use crate::{script, transaction};

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct Txid(pub ByteArray<32>);

impl std::str::FromStr for Txid {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let h = sha256d::Hash::from_str(s).map_err(|_| "invalid Txid")?;
        Ok(Self(h.to_byte_array().into()))
    }
}

impl std::fmt::Display for Txid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        sha256d::Hash::from_bytes_ref(&self.0).fmt(f)
    }
}

impl From<ByteArray<32>> for Txid {
    fn from(val: ByteArray<32>) -> Self {
        Self(val)
    }
}

impl From<[u8; 32]> for Txid {
    fn from(val: [u8; 32]) -> Self {
        Self(val.into())
    }
}

impl From<transaction::Txid> for Txid {
    fn from(txid: transaction::Txid) -> Self {
        Self((*txid).into())
    }
}

impl From<Txid> for transaction::Txid {
    fn from(txid: Txid) -> Self {
        Self::from_byte_array(*txid.0)
    }
}

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct Address(pub ByteArray<21>);

impl From<ByteArray<21>> for Address {
    fn from(val: ByteArray<21>) -> Self {
        Self(val)
    }
}

impl From<[u8; 21]> for Address {
    fn from(val: [u8; 21]) -> Self {
        Self(val.into())
    }
}

impl From<script::Address> for Address {
    fn from(addr: script::Address) -> Self {
        Self(addr.0.into())
    }
}

impl From<Address> for script::Address {
    fn from(addr: Address) -> Self {
        Self(*addr.0)
    }
}

impl std::str::FromStr for Address {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let addr = script::Address::from_str(s)?;
        Ok(Self(addr.0.into()))
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        script::Address(*self.0).fmt(f)
    }
}

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct OutPoint {
    pub txid: Txid,
    pub vout: u32,
}

impl From<transaction::OutPoint> for OutPoint {
    fn from(val: transaction::OutPoint) -> Self {
        Self {
            txid: val.txid.into(),
            vout: val.vout,
        }
    }
}

impl From<OutPoint> for transaction::OutPoint {
    fn from(val: OutPoint) -> Self {
        Self {
            txid: val.txid.into(),
            vout: val.vout,
        }
    }
}

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct Utxo {
    pub height: u64, // block height that the Tx was included in
    pub txid: Txid,
    pub vout: u32,
    pub value: u64,
}

impl From<Utxo> for transaction::TxIn {
    fn from(val: Utxo) -> Self {
        Self::with_outpoint(transaction::OutPoint {
            txid: val.txid.into(),
            vout: val.vout,
        })
    }
}

#[derive(CandidType, Clone, Debug, Default, Deserialize, Serialize)]
pub struct UnspentTx {
    pub height: u64,              // block height that the Tx was included in
    pub output: Vec<ByteBuf>,     // a list of TxOut data
    pub spent: Vec<Option<Txid>>, // a list of txid indicating whether the TxOut has been spent
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct TxStatus {
    pub height: u64, // block height that the Tx was included in
    pub tip_height: u64,
    pub confirmed_height: u64,
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct BlockRef {
    pub hash: ByteArray<32>,
    pub height: u64,
}

pub fn sha3_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct SendTxInput {
    pub tx: ByteBuf,                            // signed or unsigned transaction
    pub from_subaccount: Option<ByteArray<32>>, // should be None for signed transaction
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct SendTxOutput {
    pub txid: Txid,
    pub tip_height: u64,
    pub instructions: u64,
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct CreateTxInput {
    pub address: String,
    pub amount: u64,
    pub fee_rate: u64, // units per vByte, should >= 1000
    pub from_subaccount: Option<ByteArray<32>>,
    pub utxos: Vec<Utxo>, // optional, if not provided, will fetch from the UTXOs indexer
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct CreateTxOutput {
    pub tx: ByteBuf, // unsigned transaction
    pub fee: u64,
    pub tip_height: u64,
    pub instructions: u64,
}

#[derive(CandidType, Clone, Debug, Serialize, Deserialize)]
pub struct UtxosOutput {
    pub tip_height: u64,
    pub tip_blockhash: ByteArray<32>,
    pub confirmed_height: u64,
    pub utxos: Vec<Utxo>,
}
