use bitcoin::{
    consensus::{encode, Decodable, Encodable},
    VarInt,
};
use bitcoin_io::{Error, Read, Write};

pub mod amount;
pub mod block;
pub mod canister;
pub mod chainparams;
pub mod jsonrpc;
pub mod opcodes;
pub mod script;
pub mod sighash;
pub mod transaction;

pub extern crate hex;

pub fn consensus_encode_vec<T, W>(vv: &[T], w: &mut W) -> Result<usize, Error>
where
    T: Encodable,
    W: Write + ?Sized,
{
    let mut len = 0;
    len += VarInt::from(vv.len()).consensus_encode(w)?;
    for v in vv.iter() {
        len += v.consensus_encode(w)?;
    }
    Ok(len)
}

pub fn consensus_decode_from_vec<T, R>(r: &mut R) -> Result<Vec<T>, encode::Error>
where
    T: Decodable,
    R: Read + ?Sized,
{
    let cap: VarInt = Decodable::consensus_decode(r)?;
    let cap = cap.0 as usize;
    let mut vv = Vec::with_capacity(cap);
    for _ in 0..cap {
        vv.push(Decodable::consensus_decode_from_finite_reader(r)?);
    }
    Ok(vv)
}

pub fn err_string(err: impl std::fmt::Display) -> String {
    err.to_string()
}
