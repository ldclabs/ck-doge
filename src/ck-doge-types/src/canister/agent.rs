use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as base64_url, Engine};
use candid::CandidType;
use ciborium::into_writer;
use ic_cdk::api::management_canister::http_request::{
    http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
    TransformContext,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use super::{derive_public_key, sha3_256, sign_with, ECDSAPublicKey};
use crate::{
    err_string,
    jsonrpc::{JsonRPCAgent, APP_AGENT},
};

#[derive(CandidType, Default, Clone, Deserialize, Serialize)]
pub struct RPCAgent {
    pub name: String, // used as a prefix for idempotency_key and message in sign_proxy_token to separate different business processes.
    pub endpoint: String,
    pub max_cycles: u64,
    pub proxy_token: Option<String>,
    pub api_token: Option<String>,
}

// use Idempotent Proxy's Token: Token(pub u64, pub String, pub ByteBuf);
// https://github.com/ldclabs/idempotent-proxy/blob/main/src/idempotent-proxy-types/src/auth.rs#L15
pub async fn sign_proxy_token(
    key_name: &str,
    expire_at: u64, // UNIX timestamp, in seconds
    message: &str,  // use RPCAgent.name as message
) -> Result<String, String> {
    let mut buf: Vec<u8> = Vec::new();
    into_writer(&(expire_at, message), &mut buf).expect("failed to encode Token in CBOR format");
    let digest = sha3_256(&buf);
    let sig = sign_with(key_name, vec![b"sign_proxy_token".to_vec()], &digest)
        .await
        .map_err(err_string)?;
    buf.clear();
    into_writer(&(expire_at, message, ByteBuf::from(sig)), &mut buf).map_err(err_string)?;
    Ok(base64_url.encode(buf))
}

pub fn proxy_token_public_key(ecdsa_public_key: &ECDSAPublicKey) -> String {
    let pk = derive_public_key(ecdsa_public_key, vec![b"sign_proxy_token".to_vec()]);
    base64_url.encode(pk.public_key)
}

#[async_trait]
impl JsonRPCAgent for &RPCAgent {
    async fn post(&self, idempotency_key: &str, body: Vec<u8>) -> Result<bytes::Bytes, String> {
        let mut request_headers = vec![
            HttpHeader {
                name: "content-type".to_string(),
                value: "application/json".to_string(),
            },
            HttpHeader {
                name: "user-agent".to_string(),
                value: APP_AGENT.to_string(),
            },
            // filter out all headers except "content-type", "content-length" and "date"
            // because this 3 headers will allways be returned from the server side
            HttpHeader {
                name: "response-headers".to_string(),
                value: "date".to_string(),
            },
            HttpHeader {
                name: "idempotency-key".to_string(),
                value: idempotency_key.to_string(),
            },
        ];

        if let Some(proxy_token) = &self.proxy_token {
            request_headers.push(HttpHeader {
                name: "proxy-authorization".to_string(),
                value: format!("Bearer {}", proxy_token),
            });
        }

        if let Some(api_token) = &self.api_token {
            request_headers.push(HttpHeader {
                name: "authorization".to_string(),
                value: api_token.clone(),
            });
        }

        let request = CanisterHttpRequestArgument {
            url: self.endpoint.to_string(),
            max_response_bytes: None, //optional for request
            method: HttpMethod::POST,
            headers: request_headers,
            body: Some(body),
            transform: Some(TransformContext::from_name(
                "transform_jsonrpc".to_string(),
                vec![],
            )),
        };

        match http_request(request, self.max_cycles as u128).await {
            Ok((res,)) => {
                if res.status >= 200u64 && res.status < 300u64 {
                    Ok(bytes::Bytes::from(res.body))
                } else {
                    Err(format!(
                        "Failed to send request. status: {}, body: {}, url: {}",
                        res.status,
                        String::from_utf8(res.body).unwrap_or_default(),
                        self.endpoint,
                    ))
                }
            }
            Err((code, message)) => Err(format!(
                "The http_request resulted into error. code: {code:?}, error: {message}"
            )),
        }
    }
}

#[ic_cdk::query(hidden = true)]
fn transform_jsonrpc(args: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: args.response.status,
        body: args.response.body,
        // Remove headers (which may contain a timestamp) for consensus
        headers: vec![],
    }
}
