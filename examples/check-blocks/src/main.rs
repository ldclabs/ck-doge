use async_trait::async_trait;
use base64::Engine;
use ck_doge_types::{block::*, err_string, jsonrpc::*};
use dotenvy::dotenv;
use reqwest::{header, Client, ClientBuilder, Url};
use std::str::FromStr;
use std::time::Duration;

struct RPCAgent {
    client: Client,
    url: Url,
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
    async fn post(&self, _idempotency_key: String, body: Vec<u8>) -> Result<bytes::Bytes, String> {
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

// cargo run -p check-blocks
// cargo run -p check-blocks --release
#[tokio::main]
async fn main() {
    dotenv().expect(".env file not found");

    let agent = RPCAgent::new();
    assert!(DogecoinRPC::ping(&agent, "".to_string()).await.is_ok());

    let mut height = 5179564u64;
    let mut prev_blockhash =
        BlockHash::from_str("9679d40a4a78d0570cb010b50bf4b801532dc3e286b7e99af5289a4553e6b315")
            .unwrap();

    // let mut prev_blockhash = DogecoinRPC::get_best_blockhash(&agent, "").await.unwrap();
    // println!("best_blockhash: {:?}", prev_blockhash);

    while prev_blockhash != BlockHash::default() {
        match DogecoinRPC::get_block(&agent, prev_blockhash.to_string(), &prev_blockhash).await {
            Ok(block) => {
                println!("Block({}): {:?}", height, prev_blockhash);
                assert_eq!(block.block_hash(), prev_blockhash);

                height -= 1;
                prev_blockhash = block.header.prev_blockhash;
            }
            Err(err) => {
                println!("Block({}): {:?} Error: {:?}", height, prev_blockhash, err);
                break;
            }
        }
    }
}
