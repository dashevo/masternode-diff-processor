pub extern crate bitcoin_hashes as hashes;
pub extern crate secp256k1;

#[cfg(feature = "std")]
use std::io;
#[cfg(not(feature = "std"))]
use core2::io;

#[macro_use]
mod internal_macros;
mod common;
mod consensus;
mod crypto;
mod keys;
mod masternode;
mod transactions;
mod util;
mod blockdata;
mod network;
mod hash_types;

pub mod manager {
    use std::slice;
    use std::array::IntoIter;
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::iter::FromIterator;
    use byte::*;

    use crate::common::llmq_type::LLMQType;
    use crate::common::merkle_tree::{MerkleTree};
    use crate::consensus::Decodable;
    use crate::consensus::encode::VarInt;
    use crate::crypto::byte_util::{MNPayload, Reversable, UInt256};
    use crate::crypto::data_ops::inplace_intersection;
    use crate::masternode::masternode_list::MasternodeList;
    use crate::masternode::quorum_entry::QuorumEntry;
    use crate::masternode::masternode_entry::{MasternodeEntry};
    use crate::transactions::coinbase_transaction::CoinbaseTransaction;


    #[repr(C)]
    #[derive(Debug)]
    pub struct Result {
        pub found_coinbase: bool, //1 byte
        pub valid_coinbase: bool, //1 byte
        pub root_mn_list_valid: bool, //1 byte
        pub root_quorum_list_valid: bool, //1 byte
        pub valid_quorums: bool, //1 byte
        // pub masternode_list: Option<*mut MasternodeList>,
        // pub added_masternodes: Option<*mut HashMap<[u8; 32], MasternodeEntry>>,
        // pub modified_masternodes: Option<*mut HashMap<[u8; 32], MasternodeEntry>>,
        // pub added_quorums: Option<*mut HashMap<LLMQType, *mut HashMap<[u8; 32], QuorumEntry>>>,
        // pub needed_masternode_lists: Option<*mut HashSet<[u8; 32]>>,

        // pub value_length: usize, //8 bytes
        // pub value: *mut u8, //value_length bytes
    }

    /*#[derive(Deserialize, Debug)]
    pub struct BlockData<'a> {
        pub version: u32,
        pub hash: &'a str,
        pub previousblockhash: &'a str,
        pub merkleroot: &'a str,
        pub time: u64,
        pub bits: &'a str,
        pub chainwork: &'a str,
        pub height: u32,
    }*/

    // pub type BlockHeightLookup = unsafe extern "C" fn(block_hash: [u8; 32]) -> u32;
    // pub type MasternodeListLookup = unsafe extern "C" fn(block_hash: [u8; 32]) -> Option<MasternodeList>;
    // pub type AddInsightBlockingLookup = unsafe extern "C" fn(block_hash: [u8; 32]) -> bool;
    // pub type ShouldProceeQuorumTypeCallback = unsafe extern "C" fn(quorum_type: LLMQType) -> bool;

    pub type BlockHeightLookup = unsafe extern "C" fn(block_hash: *const u8) -> u32;
    pub type MasternodeListLookup = unsafe extern "C" fn(block_hash: *const u8) -> Option<MasternodeList<'static>>;
    pub type AddInsightBlockingLookup = unsafe extern "C" fn(block_hash: *const u8) -> bool;
    pub type ShouldProceeQuorumTypeCallback = unsafe extern "C" fn(quorum_type: LLMQType) -> bool;



    /*pub fn block_until_add_insight(block_hash: &[u8; 32], chain: Chain) {
        assert_ne!(block_hash, 0);
        let insight_url = if chain.is_main_net() {INSIGHT_URL} else {TESTNET_INSIGHT_URL};
        let path = format!("{}{}{}", insight_url, BLOCK_PATH, hex_with_data(block_hash)).as_str();
        let client = Client::new();
        let uri = path.parse()?;
        let mut resp = client.get(uri).await?;
        println!("Response: {}", resp.status());
        let mut body = Vec::new();
        while let Some(chunk) = resp.body_mut().next().await {
            body.extend_from_slice(&chunk?);
        }
        let block_data: BlockData = serde_json::from_slice(&body)?;
        chain.add_insight_verified_block(block_data);
    }*/

    fn boxed<T>(obj: T) -> *mut T {
        Box::into_raw(Box::new(obj))
    }

    fn failure() -> *mut Result {
        boxed(Result {
            found_coinbase: false,
            valid_coinbase: false,
            root_mn_list_valid: false,
            root_quorum_list_valid: false,
            valid_quorums: false,
            // masternode_list: None,
            // added_masternodes: None,
            // modified_masternodes: None,
            // added_quorums: None,
            // needed_masternode_lists: None,
        })
    }

    #[no_mangle]
    pub extern fn process_diff(
        c_array: *const u8,
        length: usize,
        base_masternode_list: Option<MasternodeList>,
        masternode_list_lookup: MasternodeListLookup,
        merkle_root: *const u8,
        use_insight_lookup: AddInsightBlockingLookup,
        should_process_quorum_of_type: ShouldProceeQuorumTypeCallback,
        block_height_lookup: BlockHeightLookup
    ) -> *mut Result {
        let message: &[u8] = unsafe { slice::from_raw_parts(c_array, length as usize) };
        let merkle_root_bytes = unsafe { slice::from_raw_parts(merkle_root, 32) };
        let desired_merkle_root = match merkle_root_bytes.read_with::<UInt256>(&mut 0, LE) {
            Ok(data) => data,
            Err(_err) => { return failure(); }
        };
        let mut offset = &mut 0;
        let _base_block_hash = match message.read_with::<UInt256>(offset, LE) {
            Ok(data) => data,
            Err(_err) => { return failure(); }
        };
        let block_hash = match message.read_with::<UInt256>(offset, LE) {
            Ok(data) => data,
            Err(_err) => { return failure(); }
        };
        let block_height = block_height_lookup(block_hash.0.as_ptr());
        let total_transactions = match message.read_with::<u32>(offset, LE) {
            Ok(data) => data,
            Err(_err) => { return failure(); }
        };
        if length - *offset < 1 { return failure(); }
        let merkle_hash_count_length = match VarInt::consensus_decode(&message[*offset..]) {
            Ok(data) => data.len(),
            Err(_err) => { return failure(); }
        };
        let merkle_hashes = &message[*offset..merkle_hash_count_length];
        *offset += merkle_hash_count_length;

        let merkle_flag_count_length = match VarInt::consensus_decode(&message[*offset..]) {
            Ok(data) => data.len(),
            Err(_err) => { return failure(); }
        };
        let merkle_flags = &message[*offset..merkle_flag_count_length];
        *offset += merkle_flag_count_length;

        let coinbase_transaction = CoinbaseTransaction::new(message);
        if coinbase_transaction.is_none() { return failure(); }
        *offset += *coinbase_transaction.unwrap().base.payload_offset;
        if length - *offset < 1 { return failure(); }
        let deleted_masternode_var_int = match VarInt::consensus_decode(&message[*offset..]) {
            Ok(data) => data,
            Err(_err) => { return failure(); }
        };
        *offset += deleted_masternode_var_int.len();
        let mut deleted_masternode_count = deleted_masternode_var_int.0.clone();
        let mut deleted_masternode_hashes: Vec<UInt256> = Vec::new();
        for _i in 0..deleted_masternode_count {
            deleted_masternode_hashes.push(match message.read_with::<UInt256>(offset, LE) {
                Ok(data) => data,
                Err(_err) => { return failure(); }
            });
        }

        let added_masternode_var_int = match VarInt::consensus_decode(&message[*offset..]) {
            Ok(data) => data,
            Err(_err) => { return failure(); }
        };
        *offset += added_masternode_var_int.len();
        let mut added_masternode_count = added_masternode_var_int.0.clone();
        let mut added_or_modified_masternodes: HashMap<UInt256, MasternodeEntry> = HashMap::with_capacity(added_masternode_count as usize);
        for _i in 0..added_masternode_count {
            if let Ok(mn_entry_payload) = message.read_with::<MNPayload>(offset, LE) {
                if let Some(mut mn_entry) = MasternodeEntry::new(mn_entry_payload, block_height) {
                    let mut key = mn_entry.provider_registration_transaction_hash.reversed();
                    added_or_modified_masternodes[&key] = mn_entry;
                }
            }
        }

        let mut added_masternodes = added_or_modified_masternodes.clone();
        let mut modified_masternode_keys: HashSet<UInt256> = HashSet::new();

        if base_masternode_list.is_some() {
            base_masternode_list.unwrap()
                .masternodes
                .into_iter()
                .for_each(|(h, e)| { added_masternodes.remove(&h); });

           let mut new_mn_keys: HashSet<UInt256> = added_or_modified_masternodes
               .clone()
               .keys()
               .cloned()
               .collect();
            let mut old_mn_keys: HashSet<UInt256> = base_masternode_list.unwrap()
                .masternodes
                .keys()
                .cloned()
                .collect();

            modified_masternode_keys = inplace_intersection(&mut new_mn_keys, &mut old_mn_keys);
        }

        let modified_masternodes: HashMap<UInt256, MasternodeEntry> =
            modified_masternode_keys
                .iter()
                .fold(HashMap::new(), |mut acc, item| {
                    acc[item] = added_or_modified_masternodes[item];
                    acc
                });

        let mut deleted_quorums: HashMap<LLMQType, Vec<UInt256>> = HashMap::new();
        let mut added_quorums: HashMap<LLMQType, HashMap<UInt256, QuorumEntry>> = HashMap::new();
        // let mut added_quorums: HashMap<LLMQType, HashMap<UInt256, *mut QuorumEntry>> = HashMap::new();

        let quorums_active = coinbase_transaction.unwrap().coinbase_transaction_version >= 2;
        let mut valid_quorums = true;
        let mut needed_masternode_lists: HashSet<UInt256> = HashSet::new();

        if quorums_active {
            // delete quorums
            if length - *offset < 1 { return failure(); }
            let deleted_quorums_var_int = match VarInt::consensus_decode(&message[*offset..]) {
                Ok(data) => data,
                Err(_err) => { return failure(); }
            };
            *offset += deleted_quorums_var_int.len();
            let mut deleted_quorums_count = deleted_quorums_var_int.0.clone();
            for _i in 0..deleted_quorums_count {
                let llmq_type = match message.read_with::<LLMQType>(offset, LE) {
                    Ok(data) => data,
                    Err(_err) => { return failure(); }
                };
                // *offset += 1;
                let llmq_hash = match message.read_with::<UInt256>(offset, LE) {
                    Ok(data) => data,
                    Err(_err) => { return failure(); }
                };
                // let llmq_type_u8: u8 = llmq_type.into();
                if deleted_quorums.contains_key(&llmq_type) {
                    deleted_quorums[&llmq_type].push(llmq_hash);
                } else {
                    deleted_quorums[&llmq_type] = vec![llmq_hash];
                }
            }

            // added quorums
            if length - *offset < 1 { return failure(); }
            let added_quorums_var_int = match VarInt::consensus_decode(&message[*offset..]) {
                Ok(data) => data,
                Err(_err) => { return failure(); }
            };
            *offset += added_quorums_var_int.len();
            let mut added_quorums_count = added_quorums_var_int.0.clone();
            for _i in 0..added_quorums_count {
                if let Some(mut potential_quorum_entry) = QuorumEntry::new(message, *offset) {
                    let entry_quorum_hash = potential_quorum_entry.quorum_hash;
                    let llmq_type = potential_quorum_entry.llmq_type;
                    if should_process_quorum_of_type(llmq_type) {
                        if let Some(quorum_masternode_list) = masternode_list_lookup(entry_quorum_hash.0.as_ptr()) {
                            valid_quorums &= potential_quorum_entry.validate_with(quorum_masternode_list, block_height_lookup);
                            if !valid_quorums {
                                println!("Invalid Quorum Found For Quorum at height {:?}", quorum_masternode_list.known_height);
                            }
                        } else if block_height_lookup(entry_quorum_hash.0.as_ptr()) != u32::MAX {
                            needed_masternode_lists.insert(entry_quorum_hash);
                        } else if use_insight_lookup(entry_quorum_hash.0.as_ptr()) {
                            // add_insight_lookup(entry_quorum_hash);
                            // block_until_add_insight(&entry_quorum_hash, chain);
                            if block_height_lookup(entry_quorum_hash.0.as_ptr()) != u32::MAX {
                                needed_masternode_lists.insert(entry_quorum_hash);
                            } else {
                                println!("Quorum masternode list not found and block not available");
                            }
                        } else {
                            println!("Quorum masternode list not found and block not available");
                        }
                    }
                    if added_quorums.contains_key(&llmq_type) {
                        added_quorums[&llmq_type][&entry_quorum_hash] = potential_quorum_entry;
                    } else {
                        added_quorums[&llmq_type] = HashMap::<UInt256, QuorumEntry>::from_iter(IntoIter::new([(entry_quorum_hash, potential_quorum_entry)]));
                    }
                }
            }
        }
        let mut masternodes =
            if let Some(list) = base_masternode_list {
                list.masternodes.clone()
                // list.masternodes.clone().into_iter().map(|(h, e)| (h, boxed(e))).collect()
            } else {
                BTreeMap::new()
            };
        for hash in deleted_masternode_hashes {
            masternodes.remove(&hash);
        }
        masternodes.extend(added_masternodes.clone());

        for (hash, mut modified) in modified_masternodes {
            let old = masternodes[&hash];
            if old.update_height < modified.update_height {
                modified.keep_info_of_previous_entry_version(old, block_height, block_hash);
            }
            masternodes[&hash] = modified;
        }
        let mut quorums: HashMap<LLMQType, HashMap<UInt256, QuorumEntry>> =
            if base_masternode_list.is_some() {
                // we need to do a deep mutable copy
                base_masternode_list.unwrap().quorums.clone()
                // (base_masternode_list.unwrap().quorums as HashMap<dyn Clone, dyn Clone>).clone()
            } else {
                HashMap::new()
            };
        for quorum_type in added_quorums.keys() {
            if !quorums.contains_key(quorum_type) {
                quorums[quorum_type] = HashMap::new();
            }
        }
        for quorum_type in quorums.keys() {
            let mut quorums_of_type = &quorums[quorum_type];
            if deleted_quorums.contains_key(quorum_type) {
                for deleted_quorum in deleted_quorums[quorum_type] {
                    quorums_of_type.remove(&deleted_quorum);
                }
            }
            if added_quorums.contains_key(quorum_type) {
                for (added_type, entry) in added_quorums[quorum_type] {
                    quorums_of_type.insert(added_type, entry);
                }
            }
        }
        let mut masternode_list = MasternodeList::new(masternodes, quorums, block_hash, block_height);
        let root_mn_list_valid = coinbase_transaction.unwrap().merkle_root_mn_list == masternode_list.masternode_merkle_root_with(block_height_lookup);
        // we need to check that the coinbase is in the transaction hashes we got back
        let coinbase_hash = coinbase_transaction.unwrap().base.tx_hash.unwrap();
        let mut found_coinbase: bool = false;
        let mut merkle_hash_offset = &mut 0;
        for _i in 0..merkle_hashes.len() {
            if let Ok(h) = merkle_hashes.read_with::<UInt256>(merkle_hash_offset, LE) {
                if h == coinbase_hash {
                    found_coinbase = true;
                    break;
                }
            }
        }
        // we also need to check that the coinbase is in the merkle block
        let merkle_tree = MerkleTree {
            tree_element_count: total_transactions,
            hashes: &merkle_hashes,
            flags: &merkle_flags,
            // hash_function: |data|sha256d::Hash::hash(data)
        };
        let mut found_coinbase: bool = false;

        boxed(Result {
            found_coinbase,
            valid_coinbase: merkle_tree.has_root(desired_merkle_root),
            root_mn_list_valid,
            root_quorum_list_valid: !quorums_active || coinbase_transaction.unwrap().merkle_root_llmq_list == masternode_list.quorum_merkle_root,
            valid_quorums,
            // masternode_list: Some(boxed(masternode_list)),
            // added_masternodes: Some(boxed(added_masternodes)),
            // modified_masternodes: Some(boxed(modified_masternodes)),
            // added_quorums: Some(boxed(added_quorums)),
            // needed_masternode_lists: Some(boxed(needed_masternode_lists)),
        })
    }
}




#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;
    use crate::common::chain_type::ChainType;
    use crate::common::llmq_type::LLMQType;

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    fn get_file_as_byte_vec(filename: &String) -> Vec<u8> {
        let mut f = fs::File::open(&filename).expect("no file found");
        let metadata = fs::metadata(&filename).expect("unable to read metadata");
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(&mut buffer).expect("buffer overflow");
        buffer
    }

    fn quorum_type_for_chain_locks(chain_type: ChainType) -> LLMQType {
        match chain_type {
            MainNet => LLMQType::Llmqtype40060,
            TestNet => LLMQType::Llmqtype5060,
            DevNet => LLMQType::Llmqtype1060
        }
    }

    /*#[test]
    fn test_mnl() {
        let chain_type = ChainType::TestNet;
        let bytes = get_file_as_byte_vec(&"ML_at_122088.dat".to_string()).as_slice();
        let c_array = bytes.as_ptr();
        let block_height = 122088;
        let result = manager::process_diff(
            c_array,
            bytes.len(),
            None,
            |block_hash| None,
            [0u8; 32].as_ptr(),
            |hash| false,
            |llmq_type| llmq_type == quorum_type_for_chain_locks(chain_type),
            |block_hash| block_height);
        println!("{:?}", result);

        let masternode_list_merkle_root: Vec<u8> = Vec::from_hex("94d0af97187af3b9311c98b1cf40c9c9849df0af55dc63b097b80d4cf6c816c5").expect("Invalid Hex String");
        // let masternode_list_merkle_root_bytes: &[u8; 32] = masternode_list_merkle_root.as_slice().as_ptr().try_into
        let equal = masternode_list_merkle_root == result.masternode_list.masternode_merkle_root;
        // let block_height: u32 = chain.height_for(block_hash);
        assert!(equal, "MNList merkle root should be valid");
        assert!(result.found_coinbase, &format!("Did not find coinbase at height {}", block_height));
        // turned off on purpose as we don't have the coinbase block
        //assert!(result.valid_coinbase, "Coinbase not valid at height {}",);
        assert!(result.root_mn_list_valid, "rootMNListValid not valid at height {}", block_height);
        assert!(result.root_quorum_list_valid, "rootQuorumListValid not valid at height {}", block_height);
        assert!(result.valid_quorums, "validQuorums not valid at height {}", block_height);
    }*/
}
