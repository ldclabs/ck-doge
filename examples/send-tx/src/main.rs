use async_trait::async_trait;
use base64::Engine;
use ck_doge_types::transaction::{OutPoint, Transaction, TxIn, TxOut, Txid};
use ck_doge_types::{
    chainparams::DOGE_TEST_NET_CHAIN, err_string, jsonrpc::*, script::*, sighash::*,
};
use dotenvy::dotenv;
use hex::test_hex_unwrap as hex;
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
    async fn post(&self, _idempotency_key: &str, body: Vec<u8>) -> Result<bytes::Bytes, String> {
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

// cargo run -p send-tx
#[tokio::main]
async fn main() {
    dotenv().expect(".env file not found");

    let agent = RPCAgent::new();
    assert!(DogecoinRPC::ping(&agent, "").await.is_ok());

    let chain = &DOGE_TEST_NET_CHAIN;

    let txid1 =
        Txid::from_str("adbf6cc9fb3ce82717565a2e10c935a9dd02503c36dab36fd9b3e768f9ae5fab").unwrap();
    let tx1 = DogecoinRPC::get_transaction(&agent, "", &txid1)
        .await
        .unwrap();
    println!("tx1: {:?}", tx1);
    let txid2 =
        Txid::from_str("374001013da664efee39b3a56919dbe009f9d1149f0bfbcd9f77c63c77284fe4").unwrap();
    let tx2 = DogecoinRPC::get_transaction(&agent, "", &txid2)
        .await
        .unwrap();
    println!("tx2: {:?}", tx2);

    let spend_addr = hash160_to_address(
        &hex!("3224fd0571314c5959a075b9946d9f7218c01667"),
        chain.p2pkh_address_prefix,
    );
    assert_eq!(spend_addr.to_string(), "nYmJMro1rtZvHWm5a4WxTE77bGYtRYrfao");
    let mut send_tx = Transaction {
        version: 1,
        lock_time: 0,
        input: vec![TxIn::default(), TxIn::default()],
        output: vec![TxOut::default()],
    };
    // 10 coins
    send_tx.input[0].prevout = OutPoint {
        txid: txid1,
        vout: 0,
    };
    // 1 coins
    send_tx.input[1].prevout = OutPoint {
        txid: txid2,
        vout: 0,
    };
    let fee = 1_000_000;
    send_tx.output[0].value = tx1.output[0].value + tx2.output[0].value - fee;
    send_tx.output[0].script_pubkey = spend_addr.to_script(chain); // send to self

    let secp = Secp256k1::new();
    let sk = decode_secretkey_wif(&std::env::var("SECRET_KEY").unwrap()).unwrap();
    let pk = sk.public_key(&secp);

    let mut sighasher = SighashCache::new(&mut send_tx);

    let sighash = sighasher
        .signature_hash(0, &tx1.output[0].script_pubkey, EcdsaSighashType::All)
        .unwrap();
    let sig = secp.sign_ecdsa(&sighash.into(), &sk);
    sighasher
        .set_input_script(
            0,
            &SighashSignature {
                signature: sig,
                sighash_type: EcdsaSighashType::All,
            },
            &pk,
        )
        .unwrap();

    let sighash = sighasher
        .signature_hash(1, &tx2.output[0].script_pubkey, EcdsaSighashType::All)
        .unwrap();
    let sig = secp.sign_ecdsa(&sighash.into(), &sk);
    sighasher
        .set_input_script(
            1,
            &SighashSignature {
                signature: sig,
                sighash_type: EcdsaSighashType::All,
            },
            &pk,
        )
        .unwrap();

    println!("signed tx: {:?}", sighasher.transaction());
    println!("signed txid: {:?}", sighasher.transaction().compute_txid());
    println!("tx size: {:?}", sighasher.transaction().size());
    assert_eq!(sighasher.transaction().size(), 339);

    let txid = DogecoinRPC::send_transaction(&agent, "", sighasher.transaction())
        .await
        .unwrap();

    println!("tx send: {:?}", txid);
    // https://sochain.com/tx/DOGETEST/d6af50cbadd243a5e33ddb494e30bca765d47d3eac5fd5309584bff6ea208343
}
