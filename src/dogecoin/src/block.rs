use bitcoin::consensus::{encode, Decodable, Encodable};
use bitcoin::hashes::{hash_newtype, sha256d, Hash};
use bitcoin::merkle_tree;
use bitcoin_io::{Error, Read, Write};
use std::ops::Deref;

use crate::transaction::Transaction;
use crate::{consensus_decode_from_vec, consensus_encode_vec};

hash_newtype! {
    /// A bitcoin block hash.
    pub struct BlockHash(sha256d::Hash);
    /// A hash of the Merkle tree branch or root for transactions.
    pub struct TxMerkleNode(sha256d::Hash);
}

impl Default for BlockHash {
    fn default() -> BlockHash {
        BlockHash(sha256d::Hash::all_zeros())
    }
}

impl Deref for BlockHash {
    type Target = [u8; 32];

    fn deref(&self) -> &[u8; 32] {
        self.0.as_byte_array()
    }
}

impl From<[u8; 32]> for BlockHash {
    fn from(data: [u8; 32]) -> BlockHash {
        BlockHash(sha256d::Hash::from_byte_array(data))
    }
}

impl Encodable for BlockHash {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, Error> {
        let mut len = 0;
        len += self.0.consensus_encode(w)?;
        Ok(len)
    }
}

impl Decodable for BlockHash {
    #[inline]
    fn consensus_decode_from_finite_reader<R: Read + ?Sized>(
        r: &mut R,
    ) -> Result<Self, encode::Error> {
        let hash: sha256d::Hash = Decodable::consensus_decode_from_finite_reader(r)?;
        Ok(BlockHash(hash))
    }
}

impl Default for TxMerkleNode {
    fn default() -> TxMerkleNode {
        TxMerkleNode(sha256d::Hash::all_zeros())
    }
}

impl Deref for TxMerkleNode {
    type Target = [u8; 32];

    fn deref(&self) -> &[u8; 32] {
        self.0.as_byte_array()
    }
}

impl Encodable for TxMerkleNode {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, Error> {
        let mut len = 0;
        len += self.0.consensus_encode(w)?;
        Ok(len)
    }
}

impl Decodable for TxMerkleNode {
    #[inline]
    fn consensus_decode_from_finite_reader<R: Read + ?Sized>(
        r: &mut R,
    ) -> Result<Self, encode::Error> {
        let hash: sha256d::Hash = Decodable::consensus_decode_from_finite_reader(r)?;
        Ok(TxMerkleNode(hash))
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct BlockHeader {
    /// Block version, now repurposed for soft fork signalling.
    pub version: u32,
    /// Reference to the previous block in the chain.
    pub prev_blockhash: BlockHash,
    /// The root hash of the merkle tree of transactions in the block.
    pub merkle_root: TxMerkleNode,
    /// The timestamp of the block, as claimed by the miner.
    pub time: u32,
    /// The target value below which the blockhash must lie.
    pub bits: u32,
    /// The nonce, selected to obtain a low enough blockhash.
    pub nonce: u32,
}

impl BlockHeader {
    /* Modifiers to the version.  */
    pub const VERSION_AUXPOW: u32 = 1 << 8;
    /** Bits above are reserved for the auxpow chain ID.  */
    pub const VERSION_CHAIN_START: u32 = 1 << 16;

    pub fn is_null(&self) -> bool {
        self.bits == 0
    }

    pub fn is_auxpow(&self) -> bool {
        (self.version & Self::VERSION_AUXPOW) != 0
    }

    pub fn chain_id(&self) -> u32 {
        self.version >> 16
    }

    pub fn is_legacy(&self) -> bool {
        self.version == 1 || self.chain_id() == 0
    }

    pub fn block_hash(&self) -> BlockHash {
        let mut enc = BlockHash::engine();
        self.consensus_encode(&mut enc)
            .expect("engines don't error");
        BlockHash::from_engine(enc)
    }
}

impl Encodable for BlockHeader {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, Error> {
        let mut len = 0;
        len += self.version.consensus_encode(w)?;
        len += self.prev_blockhash.consensus_encode(w)?;
        len += self.merkle_root.consensus_encode(w)?;
        len += self.time.consensus_encode(w)?;
        len += self.bits.consensus_encode(w)?;
        len += self.nonce.consensus_encode(w)?;
        Ok(len)
    }
}

impl Decodable for BlockHeader {
    #[inline]
    fn consensus_decode_from_finite_reader<R: Read + ?Sized>(
        r: &mut R,
    ) -> Result<Self, encode::Error> {
        Ok(BlockHeader {
            version: Decodable::consensus_decode_from_finite_reader(r)?,
            prev_blockhash: Decodable::consensus_decode_from_finite_reader(r)?,
            merkle_root: Decodable::consensus_decode_from_finite_reader(r)?,
            time: Decodable::consensus_decode_from_finite_reader(r)?,
            bits: Decodable::consensus_decode_from_finite_reader(r)?,
            nonce: Decodable::consensus_decode_from_finite_reader(r)?,
        })
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct MerkleBranch {
    pub hash: Vec<TxMerkleNode>,
    pub side_mask: u32,
}

impl Encodable for MerkleBranch {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, Error> {
        let mut len = 0;
        len += consensus_encode_vec(&self.hash, w)?;
        len += self.side_mask.consensus_encode(w)?;
        Ok(len)
    }
}

impl Decodable for MerkleBranch {
    fn consensus_decode_from_finite_reader<R: Read + ?Sized>(
        r: &mut R,
    ) -> Result<Self, encode::Error> {
        Ok(MerkleBranch {
            hash: consensus_decode_from_vec(r)?,
            side_mask: Decodable::consensus_decode_from_finite_reader(r)?,
        })
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct MerkleTx {
    pub coinbase_tx: Transaction,
    pub parent_hash: TxMerkleNode,
    pub coinbase_branch: MerkleBranch,
    pub blockchain_branch: MerkleBranch,
    pub parent_block: BlockHeader,
}

impl Encodable for MerkleTx {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, Error> {
        let mut len = 0;
        len += self.coinbase_tx.consensus_encode(w)?;
        len += self.parent_hash.consensus_encode(w)?;
        len += self.coinbase_branch.consensus_encode(w)?;
        len += self.blockchain_branch.consensus_encode(w)?;
        len += self.parent_block.consensus_encode(w)?;
        Ok(len)
    }
}

impl Decodable for MerkleTx {
    #[inline]
    fn consensus_decode_from_finite_reader<R: Read + ?Sized>(
        r: &mut R,
    ) -> Result<Self, encode::Error> {
        Ok(MerkleTx {
            coinbase_tx: Decodable::consensus_decode_from_finite_reader(r)?,
            parent_hash: Decodable::consensus_decode_from_finite_reader(r)?,
            coinbase_branch: Decodable::consensus_decode_from_finite_reader(r)?,
            blockchain_branch: Decodable::consensus_decode_from_finite_reader(r)?,
            parent_block: Decodable::consensus_decode_from_finite_reader(r)?,
        })
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Block {
    /// The block header
    pub header: BlockHeader,
    /// auxpow (if this is a merge-minded block)
    pub auxpow: Option<MerkleTx>,
    /// List of transactions contained in the block
    pub txdata: Vec<Transaction>,
}

impl Block {
    pub fn block_hash(&self) -> BlockHash {
        self.header.block_hash()
    }

    /// Checks if merkle root of header matches merkle root of the transaction list.
    pub fn check_merkle_root(&self) -> bool {
        match self.compute_merkle_root() {
            Some(merkle_root) => self.header.merkle_root == merkle_root,
            None => false,
        }
    }

    pub fn compute_merkle_root(&self) -> Option<TxMerkleNode> {
        let hashes = self
            .txdata
            .iter()
            .map(|obj| obj.compute_txid().to_raw_hash());
        merkle_tree::calculate_root(hashes).map(|h| h.into())
    }
}

impl Encodable for Block {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, Error> {
        let mut len = 0;
        len += self.header.consensus_encode(w)?;
        if let Some(ref auxpow) = self.auxpow {
            len += auxpow.consensus_encode(w)?;
        }
        len += consensus_encode_vec(&self.txdata, w)?;
        Ok(len)
    }
}

impl Decodable for Block {
    fn consensus_decode_from_finite_reader<R: Read + ?Sized>(
        r: &mut R,
    ) -> Result<Self, encode::Error> {
        let mut blk = Block {
            header: Decodable::consensus_decode_from_finite_reader(r)?,
            auxpow: None,
            txdata: Vec::new(),
        };
        if blk.header.is_auxpow() {
            blk.auxpow = Some(Decodable::consensus_decode_from_finite_reader(r)?);
        }
        blk.txdata = consensus_decode_from_vec(r)?;
        Ok(blk)
    }
}

impl From<BlockHeader> for BlockHash {
    fn from(header: BlockHeader) -> BlockHash {
        header.block_hash()
    }
}

impl From<&BlockHeader> for BlockHash {
    fn from(header: &BlockHeader) -> BlockHash {
        header.block_hash()
    }
}

impl From<Block> for BlockHash {
    fn from(block: Block) -> BlockHash {
        block.block_hash()
    }
}

impl From<&Block> for BlockHash {
    fn from(block: &Block) -> BlockHash {
        block.block_hash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex::test_hex_unwrap as hex;
    use hex::DisplayHex;
    use std::str::FromStr;

    #[test]
    fn test_blockhash() {
        let hash = "4339bc72f0820b2a28c5dabda3de47605cdcaad845516de82d685d51233e6c4d";
        let blockhash = BlockHash::from_str(hash).unwrap();
        let mut data: [u8; 32] = *blockhash.as_ref();
        assert_eq!(blockhash.to_string(), hash);
        data.reverse();
        assert_eq!(data.to_lower_hex_string(), hash);
    }

    #[test]
    fn test_first_block() {
        // block height 0
        let data = hex!("010000000000000000000000000000000000000000000000000000000000000000000000696ad20e2dd4365c7459b4a4a5af743d5e92c6da3229e6532cd605f6533f2a5b24a6a152f0ff0f1e678601000101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff1004ffff001d0104084e696e746f6e646fffffffff010058850c020000004341040184710fa689ad5023690c80f3a49c8f13f8d45b8c857fbcbc8bc4a8e4d3eb4b10f4d4604fa08dce601aaf0f470216fe1b51850b4acf21b179c45070ac7b03a9ac00000000");
        let mut rd = &data[..];
        let blk = Block::consensus_decode_from_finite_reader(&mut rd).unwrap();
        println!("Block: {:?}", blk);
        assert_eq!(blk.header.prev_blockhash, BlockHash::default());
    }

    #[test]
    fn test_block() {
        // https://dogechain.info/block/fb5f5b5b7d70e660c2c67bca8d3328afae32ae8bb4c8d6cbc42d96ff876b0859
        let data = hex!("040162000da1809fa62c133c8550bfd8b9cd10d4fe092bfde21e2878b1cb4e58af6a89bc832687bd8628066e1f0438bbb024fa0d887f8628481aa188235a13d5988b3bab2fb6dd641b3c011a0000000001000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4f03639426082f5669614254432f2cfabe6d6d2c1f86704524b309442ddaf53a0b12befc612d768b4061ef16f8c756d0ac8eae10000000000000001042e8fb01b6dd8dec3dc58ab82b00000000000000ffffffff0270084e25000000001976a914e16c28146ed4869c190b3f0bdc18d80d45f9213488ac0000000000000000266a24aa21a9edb157e30de8b33073d3306bc698a3a195af1ad403fd60039dce6949fd5c07f75400000000f8e994042c682997e6cbc8896cafc1272b0f7463e15de79c5c6dac28efc1399e07d9e698cf2defb464daad1ae9eafb4f8b322c4173aa42ebc416a266c04af1d1eac77feb6d1ecd018947547f09be57c71b5a124f8c92bb2e68f51ac188bdcc7dcd0b37b84bf014510523a0d947743663738cd9751725374a564ad84d8c2439a80297100afdaec455ff3bfd03dc6dbb2020392cffe6ad0c8aaf9df6ce6ac4fb00e81465b6291762e2637e44e6fee4da05e3f3daa1e0479795f8570389c61153ee4d2affa7b1efa60214706e02e0e2b6efb6e4a421dff1c1b3a1ce50b4cd153365c2a71619dc93c90d7ad10ee5c58343218020c497c82c43c8f1ca3a3e474eb31bea00000000040000000000000000000000000000000000000000000000000000000000000000e2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf97d24db2bfa41474bfb2f877d688fac5faa5e10a2808cf9de307370b93352e54894857d3e08918f70395d9206410fbfa942f1a889aa5ab8188ec33c2f6e207dc70800000000000020cb69a881eb9f5d7fd4bc222efd87cb2a9f5e6ffc415e53dd0e73366ab83c746d92bd765c621aba82551149c76600200f289f96e2994271ab9145d43648310d902fb6dd643ea1001a849aa3bf0a01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0603fbed490101ffffffff018ac7fc13e9000000232103b4fd74496045ca432f3662dd1a576fbfdfcc077e3497e0aecfc729b5267bd237ac000000000100000001356ef1f2548ab378221759c9a436652288772c6b0e42b5d70c302513045864e6010000006b483045022100d844ec85c95d0d8f297a34a163076fa6a64bc8f0b44b6e75e5fe2860f993784102205121108713b4e66371a71f1a600db66bb1c381da868a30f3633bfb6ca329ff00012102b1837670b30eaab952a339b0f7f0ceee93dfadafb5eeccd7cb13cdc1de6df79fffffffff02f0a9f648180900001976a91454f6fb64f14b756d118a96a57a2f9ebf4b4708fe88ac08124112001900001976a91403c56543bb94fe599bcbaf7a5e4e9276758c171088ac00000000010000000199584c3bb61a868cd67a348f5a5a01565b8d717c81f723914c2a12e555244b6f010000006b48304502210089500279112ed999d60af98451b9fcfd03cde6dac32b77b6a3e90edf724d75970220084966c28e313a09a54a13b00fd73d6e9633d4fa1ad7ea4bceec494ea8dadfa30121034082cdc9bad3f153b711fa746328890b8b82a1686b2e4237f294b97ce00f881effffffff02c04be94d180900001976a91454f6fb64f14b756d118a96a57a2f9ebf4b4708fe88ac79214001a8bb00001976a914d67eaf1153d0a8d61a0ceae9dd5e44d8116a4add88ac00000000020000000158b6fad10d8c85df9dc8bc90450ebe606457a411150b1fb9044f1b1a6db4385b010000006a47304402201237bacde5cd49df685b2d43d9e9de36479309810c0f8e048fd18c7d4cf2ba6102204408137e551870f9b0f0d3ed0d792b7c6d0ec077fea7b723e94ec35764cdbe130121026228360bf8b5898af555d4599d5855bccc9b4f5c1f356e180cc5369ce67e5a69fdffffff028093ea3b070000001976a914180c66c39e821b3db3b3c0dbd5af1c8d72f9bfd888accdab6ecb610400001976a914a7472ddbee1d36eeeb9cd3bb42974aece491f51b88ac000000000100000003607ee3f9c2c8eb4db9297d38c3ed1493fbdd257283eabf5a89999ebd5676f13f000000006a473044022045228179cc4fed581b5d9e6411402fd34c40866397094f0d97843ee257b30abc0220310c33c4452d47113c51257883c780d40928ba6b9e2bf14c234f1bd5991607e6012102ca3e8f77965b91cbe15203817cce0170bc066d5d7a9acb2070e9b3e4b2077bb8feffffff538cde51d37d84ec955410476c18c7dddaf5531a7aa07e60b75dd106ca50b447000000006b483045022100b58824fe1e036320a5cc267fe65fec100988df2a0d0ea594b945088c4945765b0220091e3dead2cec11ce5c4139f126452437b8ba8d957ae7dcee0fba60d854d46df0121026079f574275f68fd88fbd01c0af3364524b3071419dc24bddbf60003be974288feffffff7e24f4f31a3a6eaf109c3368352975c1e0ef10d175905e991553e4040b049e8c010000006b483045022100f3d369af38220916afb0c805581cd14b08ff0ee83b093c71fdeccb9b8fea197f022053e648c6a0ed5e1b42042d0e4654be8e039082c8106f147726e420cf1c986d03012103a0e805a231331c414b0423adef1ddedb23cc801acfd90ba7bd95907048a3b908feffffff02a4700106000000001976a91467f5672ce989470f4dcba16c1e930f80c03c887488acf23b03c3360000001976a91489248eee4e9d99729ebebbca00efb75ceb1ed01888acf9ed4900010000000131273f4a25824655398e16e6534d3da8099ed377b5daa7128d5fa2db5318852e010000006a473044022029d8c96e1485ccde1de68594129ca83d778464e641aee3c74ae280249b914c4602205738b295f54f6bf2efa2ea511ed9a7b529358a19f497ed0947459302685c6baa012102ac12310823256a96f52ba118a6e68d87f18026a00fb063d49486054dee5446830000000002584f6b79000000001976a9145b061aaf4a6df8c894cd2bcbca10dabeb1cad83588acc88e5d4c040000001976a9149680197eebc600376714f444554e3630e648a7b888ac00000000010000000450d3731ee755019212ea5bccac0c32a97af581030b6bb8c4233ef379c0ef958d000000006a47304402202a29e84ed9891a66e6f42599a3cb03e6ba73d2ee00febea274312d1fe49bd56f022028f0ccd587bf8eafc89c056ecd8940b7a26d19695c61c32f6eb9f495bdede870012102d490f2d12789625605be47a60f59d70cc0cb788d05e2b79120c734793b54c5adffffffff129c14196961873af798fbddb991435e1b406f2ad8a3b292114a1ead83953193010000006b483045022100809333b845790c7949fd604836b9aaa353e275de7df31fdfa13b99e8957f5194022026e610dca1e2d1c16aee4fe15a1b1ab3a66127645c8883862dcb6e475ad9ebd5012102d490f2d12789625605be47a60f59d70cc0cb788d05e2b79120c734793b54c5adffffffffb5dd9827af39cecef513370df50624a17bacd4e93b61dfe6ad71f904eb664fa4000000006a47304402206567cdad586a07b49ed969969a5caffad6339a2441e2ba69ae5ca9f5319ea88f02202a0ef72a8e7152d94a93d2aad8db8e194660b8a23ef9d360d93e49a81f5cfaf7012102d490f2d12789625605be47a60f59d70cc0cb788d05e2b79120c734793b54c5adffffffff8e7f95999bcdc77dc7c0b764043d12cd9d0bb626f554551e7253f0ee017367d5010000006a4730440220613b4e39f2604901a3efd012bf6ea54ed33ec2d95c5ba9fe40ad3a517ac1b2230220066622fc4010bd9cc4fd3509a14cb412196cfd79ec9e70813dd3d6c44eaea2bf012102d490f2d12789625605be47a60f59d70cc0cb788d05e2b79120c734793b54c5adffffffff0100ff2e31050000001976a914ac5fa8b431c89ea9fdba659361fbb1974e4e0a1088ac000000000100000002401675454bba9a7b7e01c946e9388786596f9ec74e591591108abe4239b3c364030000006a473044022044620235ebe46232e34d9e73c811ec7789eed3eb62f551a90b81fc244248a7f5022029b2e30ed92e1deab8626d88e5aff7dc8f7e8beda9646d0fcf1d1b722b7ffc870121038f1b037f7b4e99535a131c53089da233de3d4bfd8ea25b2fceb79faceab854d8ffffffffa896d9ca0ba40497edbb1ca51f4556d6485d7102f76b548a75277a62a302a350010000006b4830450221009c33286aa0ed016aa51d2a93c4dcd0d5a3a6cd001b57489e9d7d69e9edf16731022065101184651e7c88c321a4849f33269f817b7641630a97eb247ddd7e067db9af012102c2415cdf971bfebff1d74975c7bcc80384b7525586b05be63974ff386a4ac2c6ffffffff02f8507797280000001976a9148bec14087c16efa41ecadcf101568cec874fdf2588ac0aa76ee8140000001976a914205d4df4664d90e2b0118563f4fd6dc67595015a88ac000000000100000001679ccdbc1defcda02ccd1f0be0807a7180cc493e872fd2e16b7ef0220a148fb6010000006b483045022100b582e2bf7cd54c76a907d2e91f928c733f1b13b2632fa300a7ad4a923828b57702204a75316db84002135a8bc1ada81658dbb477bd86daebbd19a645701a8487e5f701210265a3c00a588c899676681e166c916ed030385c9da5a4c8d4b9029eed88a56d44ffffffff0299c039e3010000001976a9147829cc30753fe7322521f6faf77e2e4091f4766988acf9f4b8bd1a0000001976a9140be4c25349ee33a3f3d9674fdd31618918cacd4588ac000000000100000001892bc2522115e7bbf227a93aa5708e22b621616a398ab4c7119237b1da0b37d84f000000da00483045022100c5163a753aa37f9dfed708ed9167474f2b10a2b57238b00a95b3edb4f302d3fe022073fdc9c7a2147c79fb04e121a0aa64ce82901e31d15f65fa9c7a0633f2923112014730440220368808f54756249843c30cbf9ff964d1eddf4473f13352084dc1ab2229209e3302202578414b8fa035ffb58c861e965f0bcb9111636eedd173a936a4a326c138259d0147522102e60e9ac8a490768b3cd74fa55acf36d92491012c46dda806367f6dd567ac7b012103e5ed64e1c73f6d341d2a36d3f987a3b325117843f65afc24a616879047c7971652ae000000000200ca9a3b000000001976a914c7935d8f471d3472d31b6c5682f4bc2aaf2b806488acc54b563e0000000017a914f033fcfbdeafc8d9547c9015e021375694c316008700000000");
        let mut rd = &data[..];
        let blk = Block::consensus_decode_from_finite_reader(&mut rd).unwrap();
        println!("Block: {:?}\ntxdata: {:?}", blk.header, blk.txdata.len());

        // println!("Block root: {:?}", blk.auxpow);

        // serialize also asserts len is correct
        let buf = bitcoin::consensus::encode::serialize(&blk);
        assert_eq!(buf, data);

        // println!("Tx 0: {:?}", blk.txdata[0]);
        // println!("Tx m: {:?}", blk.clone().auxpow.unwrap());

        assert_eq!(
            blk.block_hash().to_string(),
            "fb5f5b5b7d70e660c2c67bca8d3328afae32ae8bb4c8d6cbc42d96ff876b0859"
        );
        assert_eq!(blk.header.version, 6422788);
        assert_eq!(
            blk.header.prev_blockhash.to_string(),
            "bc896aaf584ecbb178281ee2fd2b09fed410cdb9d8bf50853c132ca69f80a10d"
        );
        assert_eq!(
            blk.header.merkle_root.to_string(),
            "ab3b8b98d5135a2388a11a4828867f880dfa24b0bb38041f6e062886bd872683"
        );
        assert_eq!(blk.header.time, 1692251695); // 2023-08-17 05:54:55 UTC
        assert_eq!(blk.header.bits, 0x1a013c1b);
        assert_eq!(blk.header.nonce, 0);
        assert_eq!(blk.txdata.len(), 10);
        assert!(blk.header.is_auxpow());
        assert!(blk.check_merkle_root());

        let coins: Vec<u64> = blk
            .txdata
            .iter()
            .flat_map(|tx| tx.output.iter().map(|out| out.value))
            .collect();

        assert_eq!(
            coins,
            vec![
                1001062713226,
                9999907990000,
                27488096948744,
                9999991000000,
                206330249879929,
                31070000000,
                4818071366605,
                100757668,
                235200003058,
                2037075800,
                18461069000,
                22300000000,
                174339870968,
                89798911754,
                8107180185,
                114852164857,
                1000000000,
                1045842885
            ]
        );

        for (j, tx) in blk.txdata.iter().enumerate() {
            for (i, inp) in tx.input.iter().enumerate() {
                let script = inp.script.clone();
                println!("Input {}-{}: {} {:?}", j, i, inp.prevout.vout, script);
            }
            println!("Tx: {}", tx.to_bytes().as_hex());
            // for (i, out) in tx.output.iter().enumerate() {
            //     let script = ScriptBuf::from(out.script.clone());
            //     println!("Output {}-{}: {} {:?}", j, i, out.value, script);
            // }
        }
    }
}
