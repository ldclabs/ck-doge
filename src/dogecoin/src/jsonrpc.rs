use async_trait::async_trait;
use bitcoin::consensus::{Decodable, Encodable, ReadExt};
use hex::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{to_vec, Value};
use std::str::FromStr;

use crate::block::{Block, BlockHash};
use crate::err_string;
use crate::transaction::{Transaction, Txid};

pub static APP_AGENT: &str = concat!(
    "Mozilla/5.0 ck-doge ",
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
);

#[async_trait]
pub trait JsonRPCAgent {
    async fn post(&self, idempotency_key: String, body: Vec<u8>) -> Result<bytes::Bytes, String>;
}

pub struct DogecoinRPC {}

#[derive(Debug, Serialize)]
pub struct RPCRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: &'a [Value],
    id: u64,
}

#[derive(Debug, Deserialize)]
pub struct RPCResponse<T> {
    result: Option<T>,
    error: Option<Value>,
    // id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockRef {
    pub hash: String,
    pub height: u64,
}

impl From<&BlockRef> for BlockHash {
    fn from(block: &BlockRef) -> Self {
        BlockHash::from_str(&block.hash).expect("invalid block hash")
    }
}

impl DogecoinRPC {
    pub async fn ping(agent: impl JsonRPCAgent, idempotency_key: String) -> Result<(), String> {
        Self::call(agent, idempotency_key, "ping", &[]).await
    }

    pub async fn get_best_blockhash(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
    ) -> Result<BlockHash, String> {
        let hex: String = Self::call(agent, idempotency_key, "getbestblockhash", &[]).await?;
        BlockHash::from_str(&hex).map_err(err_string)
    }

    pub async fn get_blockhash(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        height: u64,
    ) -> Result<BlockHash, String> {
        let hex: String =
            Self::call(agent, idempotency_key, "getblockhash", &[height.into()]).await?;
        BlockHash::from_str(&hex).map_err(err_string)
    }

    pub async fn get_block(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        hash: &BlockHash,
    ) -> Result<Block, String> {
        let hex: String = Self::call(
            agent,
            idempotency_key,
            "getblock",
            &[hash.to_string().into(), 0.into()],
        )
        .await?;
        deserialize_hex(&hex)
    }

    pub async fn wait_for_new_block(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        timeout_ms: u64,
    ) -> Result<BlockRef, String> {
        Self::call(
            agent,
            idempotency_key,
            "waitfornewblock",
            &[timeout_ms.into()],
        )
        .await
    }

    pub async fn get_transaction(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        hash: &Txid,
    ) -> Result<Transaction, String> {
        let hex: String = Self::call(
            agent,
            idempotency_key,
            "getrawtransaction",
            &[hash.to_string().into(), 0.into()],
        )
        .await?;
        deserialize_hex(&hex)
    }

    pub async fn send_transaction(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        tx: &Transaction,
    ) -> Result<Txid, String> {
        let hex: String = Self::call(
            agent,
            idempotency_key,
            "sendrawtransaction",
            &[serialize_hex(tx).into()],
        )
        .await?;
        Txid::from_str(&hex).map_err(err_string)
    }

    pub async fn send_rawtransaction(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        raw: &[u8],
    ) -> Result<Txid, String> {
        let hex: String = Self::call(
            agent,
            idempotency_key,
            "sendrawtransaction",
            &[raw.to_lower_hex_string().into()],
        )
        .await?;
        Txid::from_str(&hex).map_err(err_string)
    }

    pub async fn call<T: DeserializeOwned>(
        agent: impl JsonRPCAgent,
        idempotency_key: String,
        method: &str,
        params: &[Value],
    ) -> Result<T, String> {
        let input = RPCRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };
        let input = to_vec(&input).map_err(err_string)?;
        let data = agent.post(idempotency_key, input).await?;

        let output: RPCResponse<T> = serde_json::from_slice(&data).map_err(err_string)?;

        if let Some(error) = output.error {
            return Err(serde_json::to_string(&error).map_err(err_string)?);
        }

        match output.result {
            Some(result) => Ok(result),
            None => serde_json::from_value(Value::Null).map_err(err_string),
        }
    }
}

pub fn serialize_hex<T: Encodable>(v: &T) -> String {
    let mut buf = Vec::new();
    v.consensus_encode(&mut buf)
        .expect("serialize_hex: encode failed");
    buf.to_lower_hex_string()
}

pub fn deserialize_hex<T: Decodable>(hex: &str) -> Result<T, String> {
    let data = Vec::from_hex(hex).map_err(err_string)?;
    let mut reader = &data[..];
    let object = Decodable::consensus_decode_from_finite_reader(&mut reader).map_err(err_string)?;
    if reader.read_u8().is_ok() {
        Err("decode_hex: data not consumed entirely".to_string())
    } else {
        Ok(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use dotenvy::dotenv;
    use reqwest::{header, Client, ClientBuilder, Url};
    use std::time::Duration;

    pub struct RPCAgent {
        client: Client,
        url: Url,
    }

    impl Default for RPCAgent {
        fn default() -> Self {
            Self::new()
        }
    }

    impl RPCAgent {
        pub fn new() -> Self {
            let rpcurl = std::env::var("RPC_URL").unwrap();
            let rpcuser = std::env::var("RPC_USER").unwrap_or_default();
            let rpcpassword = std::env::var("RPC_PASSWORD").unwrap_or_default();

            let mut common_headers = header::HeaderMap::with_capacity(4);
            common_headers.insert(header::ACCEPT, "application/json".parse().unwrap());
            common_headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            common_headers.insert(header::ACCEPT_ENCODING, "gzip".parse().unwrap());

            let url = reqwest::Url::parse(&rpcurl).unwrap();
            if !rpcuser.is_empty() && !rpcpassword.is_empty() {
                let auth = format!("{}:{}", rpcuser, rpcpassword);
                let auth = format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(auth)
                );
                common_headers.insert(header::AUTHORIZATION, auth.parse().unwrap());
            }

            let client = ClientBuilder::new()
                .use_rustls_tls()
                .no_proxy()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .user_agent(APP_AGENT)
                .default_headers(common_headers)
                .gzip(true)
                .build()
                .unwrap();

            Self { client, url }
        }
    }

    #[async_trait]
    impl JsonRPCAgent for &RPCAgent {
        async fn post(
            &self,
            _idempotency_key: String,
            body: Vec<u8>,
        ) -> Result<bytes::Bytes, String> {
            let req = self.client.post(self.url.clone()).body(body);
            let res = req.send().await.map_err(err_string)?;
            if res.status().is_success() {
                res.bytes().await.map_err(err_string)
            } else {
                Err(format!(
                    "HTTP error: {}, {}",
                    res.status(),
                    res.text().await.unwrap()
                ))
            }
        }
    }

    #[test]
    fn consensus_decode_hex_works() {
        let tx: Transaction = deserialize_hex(
            "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff060340420f0103ffffffff0136bf775ee9000000232102a1b03b741b2f25549de39ee08df766395857b2459edf28675bddb119784c7db7ac00000000",
        )
        .unwrap();
        println!("tx: {:?}", tx);
        assert_eq!(
            tx.compute_txid().to_string(),
            "bc06dcc8c8841728b905fa45e4d21ed460a2e136bb0545fcf2906a149e704bb9"
        );

        let res: Result<Transaction, _> = deserialize_hex(
            "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff060340420f0103ffffffff0136bf775ee9000000232102a1b03b741b2f25549de39ee08df766395857b2459edf28675bddb119784c7db7ac00000000ff",
        );
        assert!(res.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn rpc_works() {
        dotenv().expect(".env file not found");

        let agent = RPCAgent::new();
        assert!(DogecoinRPC::ping(&agent, "".to_string()).await.is_ok());

        let best_blockhash = DogecoinRPC::get_best_blockhash(&agent, "".to_string())
            .await
            .unwrap();
        println!("best_blockhash: {:?}", best_blockhash);

        let block = DogecoinRPC::get_block(&agent, "".to_string(), &best_blockhash)
            .await
            .unwrap();
        println!("block: {:?}", block);
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn check_some_blocks() {
        dotenv().expect(".env file not found");

        let agent = RPCAgent::new();
        assert!(DogecoinRPC::ping(&agent, "".to_string()).await.is_ok());

        let mut height = 5240740u64;
        let mut prev_blockhash =
            BlockHash::from_str("62bba02e84a5b437fcccfacbec309623bfc7479f033e250f93339f981b4ca6c4")
                .unwrap();

        // let mut prev_blockhash = DogecoinRPC::get_best_blockhash(&agent, "").await.unwrap();
        // println!("best_blockhash: {:?}", prev_blockhash);

        let mut i = 10;
        while i > 0 && prev_blockhash != BlockHash::default() {
            i -= 1;
            match DogecoinRPC::get_block(&agent, "".to_string(), &prev_blockhash).await {
                Ok(block) => {
                    println!("Block({}): {:?}", height, prev_blockhash);
                    height -= 1;
                    prev_blockhash = block.header.prev_blockhash;
                }
                Err(err) => {
                    println!("Block({}) Error: {:?}", height, err);
                    break;
                }
            }
        }
    }
}
