use ic_cdk::api::management_canister::ecdsa;
use ic_crypto_extended_bip32::{DerivationIndex, DerivationPath, ExtendedBip32DerivationOutput};
use icrc_ledger_types::icrc1::account::Account;

pub type ECDSAPublicKey = ecdsa::EcdsaPublicKeyResponse;

/// Returns the derivation path that should be used to sign a message from a
/// specified account.
pub fn account_path(account: &Account) -> Vec<Vec<u8>> {
    const SCHEMA_V1: u8 = 1;
    vec![
        vec![SCHEMA_V1],
        account.owner.as_slice().to_vec(),
        account.effective_subaccount().to_vec(),
    ]
}

/// Returns a valid extended BIP-32 derivation path from an Account (Principal + subaccount)
pub fn derive_public_key(
    ecdsa_public_key: &ECDSAPublicKey,
    derivation_path: Vec<Vec<u8>>,
) -> ECDSAPublicKey {
    let ExtendedBip32DerivationOutput {
        derived_public_key,
        derived_chain_code,
    } = DerivationPath::new(derivation_path.into_iter().map(DerivationIndex).collect())
        .public_key_derivation(&ecdsa_public_key.public_key, &ecdsa_public_key.chain_code)
        .expect("bug: failed to derive an ECDSA public key from valid inputs");
    ECDSAPublicKey {
        public_key: derived_public_key,
        chain_code: derived_chain_code,
    }
}

pub async fn sign_with(
    key_name: &str,
    derivation_path: Vec<Vec<u8>>,
    message_hash: [u8; 32],
) -> Result<Vec<u8>, String> {
    let args = ecdsa::SignWithEcdsaArgument {
        message_hash: message_hash.to_vec(),
        derivation_path,
        key_id: ecdsa::EcdsaKeyId {
            curve: ecdsa::EcdsaCurve::Secp256k1,
            name: key_name.to_string(),
        },
    };

    let (response,): (ecdsa::SignWithEcdsaResponse,) = ecdsa::sign_with_ecdsa(args)
        .await
        .map_err(|err| format!("sign_with_ecdsa failed {:?}", err))?;

    Ok(response.signature)
}

pub async fn public_key_with(
    key_name: &str,
    derivation_path: Vec<Vec<u8>>,
) -> Result<ECDSAPublicKey, String> {
    let args = ecdsa::EcdsaPublicKeyArgument {
        canister_id: None,
        derivation_path,
        key_id: ecdsa::EcdsaKeyId {
            curve: ecdsa::EcdsaCurve::Secp256k1,
            name: key_name.to_string(),
        },
    };

    let (response,): (ecdsa::EcdsaPublicKeyResponse,) = ecdsa::ecdsa_public_key(args)
        .await
        .map_err(|err| format!("ecdsa_public_key failed {:?}", err))?;

    Ok(response)
}
