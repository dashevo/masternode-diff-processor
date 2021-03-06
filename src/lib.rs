#![allow(dead_code)]
#![allow(unused_variables)]
pub extern crate bitcoin_hashes as hashes;
pub extern crate secp256k1;

#[cfg(feature = "std")]
use std::io;
#[cfg(not(feature = "std"))]
use core2::io;

#[macro_use]
pub mod internal_macros;
pub mod common;
pub mod consensus;
pub mod crypto;
pub mod masternode;
pub mod transactions;
pub mod util;
pub mod blockdata;
pub mod network;
pub mod hash_types;
pub mod ffi;

use std::slice;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::c_void;
use byte::*;
use ffi::wrapped_types;
use crate::common::block_data::BlockData;
use crate::common::llmq_type::LLMQType;
use crate::common::merkle_tree::MerkleTree;
use crate::consensus::Decodable;
use crate::consensus::encode::VarInt;
use crate::crypto::byte_util::{Data, MNPayload, Reversable, UInt256, Zeroable};
use crate::crypto::data_ops::inplace_intersection;
use crate::masternode::masternode_list::MasternodeList;
use crate::masternode::quorum_entry::QuorumEntry;
use crate::masternode::masternode_entry::MasternodeEntry;
use crate::transactions::coinbase_transaction::CoinbaseTransaction;
use ffi::wrapped_types::{AddInsightBlockingLookup, BlockHeightLookup, MasternodeListDestroy, MasternodeListLookup, MndiffResult, QuorumValidationData, ShouldProcessQuorumTypeCallback, ValidateQuorumCallback};
use crate::ffi::boxer::{boxed, boxed_vec};
use crate::ffi::from::FromFFI;
use crate::ffi::to::{encode_masternodes_map, encode_quorums_map, ToFFI};
use crate::ffi::unboxer::{unbox_any, unbox_quorum_validation_data, unbox_result};

fn failure<'a>() -> *mut MndiffResult {
    boxed(MndiffResult::default())
}

#[no_mangle]
pub extern "C" fn mndiff_process(
    message_arr: *const u8,
    message_length: usize,
    base_masternode_list: *const wrapped_types::MasternodeList,
    masternode_list_lookup: MasternodeListLookup,
    masternode_list_destroy: MasternodeListDestroy,
    merkle_root: *const u8,
    use_insight_as_backup: bool,
    add_insight_lookup: AddInsightBlockingLookup,
    should_process_quorum_of_type: ShouldProcessQuorumTypeCallback,
    validate_quorum_callback: ValidateQuorumCallback,
    block_height_lookup: BlockHeightLookup,
    context: *const c_void, // External Masternode Manager Diff Message Context ()
) -> *mut MndiffResult {
    let message: &[u8] = unsafe { slice::from_raw_parts(message_arr, message_length as usize) };
    let merkle_root_bytes = unsafe { slice::from_raw_parts(merkle_root, 32) };
    let base_masternode_list = if !base_masternode_list.is_null() {
        unsafe { Some((*base_masternode_list).decode()) }
    } else {
        None
    };
    let desired_merkle_root = match merkle_root_bytes.read_with::<UInt256>(&mut 0, LE) {
        Ok(data) => data,
        Err(_err) => { return failure(); }
    };
    let offset = &mut 0;
    let _base_block_hash = match message.read_with::<UInt256>(offset, LE) {
        Ok(data) => data,
        Err(_err) => { return failure(); }
    };
    let block_hash = match message.read_with::<UInt256>(offset, LE) {
        Ok(data) => data,
        Err(_err) => { return failure(); }
    };
    let block_height = unsafe { block_height_lookup(boxed(block_hash.0), context) };
    let total_transactions = match message.read_with::<u32>(offset, LE) {
        Ok(data) => data,
        Err(_err) => { return failure(); }
    };
    let merkle_hash_var_int = match VarInt::consensus_decode(&message[*offset..]) {
        Ok(data) => data,
        Err(_err) => { return failure(); }
    };
    let merkle_hash_count_length = merkle_hash_var_int.len();
    *offset += merkle_hash_count_length;
    let merkle_hashes_count = (merkle_hash_var_int.0 as usize) * 32;
    let merkle_hashes = &message[*offset..*offset + merkle_hashes_count];
    *offset += merkle_hashes_count;
    let merkle_flag_var_int = match VarInt::consensus_decode(&message[*offset..]) {
        Ok(data) => data,
        Err(_err) => { return failure(); }
    };
    let merkle_flag_count_length = merkle_flag_var_int.len();
    *offset += merkle_flag_count_length;

    let merkle_flag_count = merkle_flag_var_int.0 as usize;
    let merkle_flags = &message[*offset..*offset + merkle_flag_count];
    *offset += merkle_flag_count;

    let coinbase_transaction = CoinbaseTransaction::new(&message[*offset..]);
    if coinbase_transaction.is_none() { return failure(); }
    let coinbase_transaction = coinbase_transaction.unwrap();

    *offset += coinbase_transaction.base.payload_offset;
    if message_length - *offset < 1 { return failure(); }
    let deleted_masternode_var_int = match VarInt::consensus_decode(&message[*offset..]) {
        Ok(data) => data,
        Err(_err) => { return failure(); }
    };
    *offset += deleted_masternode_var_int.len();
    let deleted_masternode_count = deleted_masternode_var_int.0.clone();
    let mut deleted_masternode_hashes: Vec<UInt256> = Vec::with_capacity(deleted_masternode_count as usize);
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
    let added_masternode_count = added_masternode_var_int.0.clone();
    let added_or_modified_masternodes: BTreeMap<UInt256, MasternodeEntry> = (0..added_masternode_count)
        .into_iter()
        .map(|_i| message.read_with::<MNPayload>(offset, LE))
        .filter(|payload| payload.is_ok())
        .map(|payload| MasternodeEntry::new(payload.unwrap(), block_height))
        .filter(|entry| entry.is_some())
        .fold(BTreeMap::new(),|mut acc, entry| {
            let mn_entry = entry.unwrap();
            let hash = mn_entry.provider_registration_transaction_hash.clone().reversed();
            acc.insert(hash, mn_entry);
            acc
        });
    let mut added_masternodes = added_or_modified_masternodes.clone();
    let mut modified_masternode_keys: HashSet<UInt256> = HashSet::new();
    let (has_old, old_masternodes, old_quorums) =
        if base_masternode_list.is_some() {
            let list = base_masternode_list.unwrap();
            (true, list.masternodes, list.quorums)
        } else {
            (false, BTreeMap::new(), HashMap::new())
        };
    if has_old {
        let base_masternodes = old_masternodes.clone();
        base_masternodes
            .iter()
            .for_each(|(h, _e)| { added_masternodes.remove(h); });

        let mut new_mn_keys: HashSet<UInt256> = added_or_modified_masternodes
            .keys()
            .cloned()
            .collect();
        let mut old_mn_keys: HashSet<UInt256> = base_masternodes
            .keys()
            .cloned()
            .collect();
        modified_masternode_keys = inplace_intersection(&mut new_mn_keys, &mut old_mn_keys);
    }

    let mut modified_masternodes: BTreeMap<UInt256, MasternodeEntry> = modified_masternode_keys
        .clone()
        .into_iter()
        .fold(BTreeMap::new(), |mut acc, hash| {
            // println!("modify mnode {:?}:{:?}", hash.clone(), added_or_modified_masternodes[&hash].clone().masternode_entry_hash);
            acc.insert(hash, added_or_modified_masternodes[&hash].clone());
            acc
        });

    let mut deleted_quorums: HashMap<LLMQType, Vec<UInt256>> = HashMap::new();
    let mut added_quorums: HashMap<LLMQType, HashMap<UInt256, QuorumEntry>> = HashMap::new();

    let quorums_active = coinbase_transaction.coinbase_transaction_version >= 2;
    let mut has_valid_quorums = true;
    let mut needed_masternode_lists: Vec<*mut [u8; 32]> = Vec::new();

    if quorums_active {
        // deleted quorums
        if message_length - *offset < 1 { return failure(); }
        let deleted_quorums_var_int = match VarInt::consensus_decode(&message[*offset..]) {
            Ok(data) => data,
            Err(_err) => { return failure(); }
        };
        *offset += deleted_quorums_var_int.len();
        let deleted_quorums_count = deleted_quorums_var_int.0.clone();
        for _i in 0..deleted_quorums_count {
            let llmq_type = match message.read_with::<LLMQType>(offset, LE) {
                Ok(data) => data,
                Err(_err) => { return failure(); }
            };
            let llmq_hash = match message.read_with::<UInt256>(offset, LE) {
                Ok(data) => data,
                Err(_err) => { return failure(); }
            };
            if deleted_quorums.contains_key(&llmq_type) {
                deleted_quorums.get_mut(&llmq_type).unwrap().push(llmq_hash);
            } else {
                deleted_quorums.insert(llmq_type, vec![llmq_hash]);
            }
        }

        // added quorums
        if message_length - *offset < 1 { return failure(); }
        let added_quorums_var_int = match VarInt::consensus_decode(&message[*offset..]) {
            Ok(data) => data,
            Err(_err) => { return failure(); }
        };
        *offset += added_quorums_var_int.len();
        let added_quorums_count = added_quorums_var_int.0.clone();
        for _i in 0..added_quorums_count {
            if let Some(mut quorum_entry) = QuorumEntry::new(message, *offset) {
                *offset += quorum_entry.length;
                let quorum_hash = quorum_entry.quorum_hash;
                let llmq_type = quorum_entry.llmq_type;
                let should_process_quorum = unsafe { should_process_quorum_of_type(llmq_type.into(), context) };
                if should_process_quorum {
                    let lookup_result = unsafe { masternode_list_lookup(boxed(quorum_hash.0), context) };
                    if !lookup_result.is_null() {
                        let quorum_masternode_list = unsafe { (*lookup_result).decode() };
                        unsafe { masternode_list_destroy(lookup_result); }
                        let block_height: u32 = unsafe { block_height_lookup(boxed(quorum_masternode_list.block_hash.0), context) };
                        let valid_masternodes = quorum_masternode_list.valid_masternodes_for(quorum_entry.llmq_quorum_hash(), llmq_type.quorum_size(), block_height);
                        let operator_pks: Vec<*mut [u8; 48]> = (0..valid_masternodes.len())
                            .into_iter()
                            .filter_map(|i| match quorum_entry.signers_bitset.bit_is_true_at_le_index(i as u32) {
                                true => Some(boxed(valid_masternodes[i].operator_public_key_at(block_height).0)),
                                false => None
                            })
                            .collect();
                        let operator_public_keys_count = operator_pks.len();
                        let is_valid_signature = unsafe {
                            validate_quorum_callback(
                                boxed(QuorumValidationData {
                                    items: boxed_vec(operator_pks),
                                    count: operator_public_keys_count,
                                    commitment_hash: boxed(quorum_entry.generate_commitment_hash().0),
                                    all_commitment_aggregated_signature: boxed(quorum_entry.all_commitment_aggregated_signature.0),
                                    quorum_threshold_signature: boxed(quorum_entry.quorum_threshold_signature.0),
                                    quorum_public_key: boxed(quorum_entry.quorum_public_key.0)
                                }),
                                context
                            )
                        };
                        has_valid_quorums &= quorum_entry.validate_payload() && is_valid_signature;
                        if has_valid_quorums {
                            quorum_entry.verified = true;
                        }
                    } else {
                        if unsafe { block_height_lookup(boxed(quorum_hash.0), context) != u32::MAX } {
                            needed_masternode_lists.push(boxed(quorum_hash.0));
                        } else {
                            if use_insight_as_backup {
                                unsafe { add_insight_lookup(boxed(quorum_hash.0), context) };
                                if unsafe { block_height_lookup(boxed(quorum_hash.0), context) != u32::MAX } {
                                    needed_masternode_lists.push(boxed(quorum_hash.0));
                                }
                            }
                        }
                    }
                }
                added_quorums
                    .entry(llmq_type)
                    .or_insert(HashMap::new())
                    .insert(quorum_hash, quorum_entry);
            }
        }
    }

    let mut masternodes = if has_old {
        let mut old_mnodes = old_masternodes.clone();
        for hash in deleted_masternode_hashes {
            old_mnodes.remove(&hash.clone().reversed());
        }
        old_mnodes.extend(added_masternodes.clone());
        old_mnodes
    } else {
        added_masternodes.clone()
    };

    modified_masternodes.iter_mut().for_each(|(hash, modified)| {
        if let Some(mut old) = masternodes.get_mut(hash) {
            let new_height = (*modified).update_height;
            if (*old).update_height < new_height {
                let b = BlockData { height: block_height, hash: block_hash };
                let new_pro_reg_tx_hash = (*modified).provider_registration_transaction_hash;
                if new_pro_reg_tx_hash == (*old).provider_registration_transaction_hash {
                    (*modified).previous_validity = (*old)
                        .previous_validity
                        .clone()
                        .into_iter()
                        .filter(|(block, _)| block.height < new_height)
                        .collect();
                    if (*old).is_valid_at(new_height) != (*modified).is_valid {
                        (*modified).previous_validity.insert(b.clone(), (*old).is_valid.clone());
                    }
                    (*modified).previous_operator_public_keys = (*old)
                        .previous_operator_public_keys
                        .clone()
                        .into_iter()
                        .filter(|(block, _)| block.height < new_height)
                        .collect();
                    if (*old).operator_public_key_at(new_height) != (*modified).operator_public_key {
                        (*modified).previous_operator_public_keys.insert(b.clone(), (*old).operator_public_key.clone());
                    }
                    let old_prev_mn_entry_hashes = (*old)
                        .previous_masternode_entry_hashes
                        .clone()
                        .into_iter()
                        .filter(|(block, _)| (*block).height < new_height)
                        .collect();
                    (*modified).previous_masternode_entry_hashes = old_prev_mn_entry_hashes;
                    if (*old).masternode_entry_hash_at(new_height) != (*modified).masternode_entry_hash {
                        (*modified).previous_masternode_entry_hashes.insert(b.clone(), (*old).masternode_entry_hash.clone());
                    }
                }

                if !(*old).confirmed_hash.is_zero() &&
                    (*old).known_confirmed_at_height.is_some() &&
                    (*old).known_confirmed_at_height.unwrap() > block_height {
                    (*old).known_confirmed_at_height = Some(block_height);
                }
            }
            masternodes.insert((*hash).clone(), (*modified).clone());
        }
    });

    let mut quorums = old_quorums.clone();

    let quorums_to_add = added_quorums
        .clone()
        .into_iter()
        .filter(|(key, _entries)| !quorums.contains_key(key))
        .collect::<HashMap<LLMQType, HashMap<UInt256, QuorumEntry>>>();

    quorums.extend(quorums_to_add);

    quorums.iter_mut().for_each(|(quorum_type, quorums_map)| {
        if let Some(keys_to_delete) = deleted_quorums.get(quorum_type) {
            keys_to_delete.into_iter().for_each(|key| {
                (*quorums_map).remove(key);
            });
        }
        if let Some(keys_to_add) = added_quorums.get(quorum_type) {
            keys_to_add.clone().into_iter().for_each(|(key, entry)| {
                (*quorums_map).insert(key, entry);
            });
        }
    });
    let masternode_list = MasternodeList::new(masternodes, quorums, block_hash, block_height, quorums_active);

    let has_valid_mn_list_root =
        if let Some(mn_merkle_root) = masternode_list.masternode_merkle_root {
            println!("rootMNListValid: {:?} == {:?}", mn_merkle_root, coinbase_transaction.merkle_root_mn_list);
            coinbase_transaction.merkle_root_mn_list == mn_merkle_root
        } else {
            false
        };
    // we need to check that the coinbase is in the transaction hashes we got back
    let coinbase_hash = coinbase_transaction.base.tx_hash.unwrap();
    let mut has_found_coinbase: bool = false;
    let merkle_hash_offset = &mut 0;
    for _i in 0..merkle_hashes.len() {
        if let Ok(h) = merkle_hashes.read_with::<UInt256>(merkle_hash_offset, LE) {
            println!("finding coinbase: {:?} == {:?}", coinbase_hash, h);
            if h == coinbase_hash {
                has_found_coinbase = true;
                break;
            }
        }
    }
    // we also need to check that the coinbase is in the merkle block
    let merkle_tree = MerkleTree {
        tree_element_count: total_transactions,
        hashes: merkle_hashes,
        flags: merkle_flags,
    };

    let mut has_valid_quorum_list_root = true;
    if quorums_active {
        let q_merkle_root = masternode_list.quorum_merkle_root;
        let ct_q_merkle_root = coinbase_transaction.merkle_root_llmq_list;
        println!("rootQuorumListValid: {:?} == {:?}", q_merkle_root, ct_q_merkle_root);
        has_valid_quorum_list_root =
            q_merkle_root.is_some() &&
                ct_q_merkle_root.is_some() &&
                ct_q_merkle_root.unwrap() == q_merkle_root.unwrap();
        if !has_valid_quorum_list_root {
            println!("Quorum Merkle root not valid for DML on block {} version {} ({:?} wanted - {:?} calculated)",
                     coinbase_transaction.height,
                     coinbase_transaction.base.version,
                     coinbase_transaction.merkle_root_llmq_list,
                     masternode_list.quorum_merkle_root);
        }
    }
    let has_valid_coinbase = merkle_tree.has_root(desired_merkle_root);

    let added_masternodes_count = added_masternodes.len();
    let added_masternodes = encode_masternodes_map(&added_masternodes);
    let modified_masternodes_count = modified_masternodes.len();
    let modified_masternodes = encode_masternodes_map(&modified_masternodes);
    let added_quorum_type_maps_count = added_quorums.len();
    let added_quorum_type_maps = encode_quorums_map(&added_quorums);

    let needed_masternode_lists_count = needed_masternode_lists.len();
    let needed_masternode_lists = boxed_vec(needed_masternode_lists);

    let masternode_list = boxed(masternode_list.encode());

    let result = MndiffResult {
        has_found_coinbase,
        has_valid_coinbase,
        has_valid_mn_list_root,
        has_valid_quorum_list_root,
        has_valid_quorums,
        masternode_list,
        added_masternodes,
        added_masternodes_count,
        modified_masternodes,
        modified_masternodes_count,
        added_quorum_type_maps,
        added_quorum_type_maps_count,
        needed_masternode_lists,
        needed_masternode_lists_count
    };
    boxed(result)
}

#[no_mangle]
pub unsafe extern fn mndiff_block_hash_destroy(block_hash: *mut [u8; 32]) {
    unbox_any(block_hash);
}

#[no_mangle]
pub unsafe extern fn mndiff_quorum_validation_data_destroy(data: *mut QuorumValidationData) {
    unbox_quorum_validation_data(data);
}

#[no_mangle]
pub unsafe extern fn mndiff_destroy(result: *mut MndiffResult) {
    unbox_result(result);
}


#[cfg(test)]
mod tests {
    use std::{env, fs};
    use std::ffi::c_void;
    use std::io::Read;
    use std::ptr::null_mut;
    use byte::{BytesExt, LE};
    use hashes::hex::{FromHex, ToHex};
    use crate::common::chain_type::ChainType;
    use crate::common::llmq_type::LLMQType;
    use crate::crypto::byte_util::{Reversable, UInt256};
    use crate::ffi::from::FromFFI;
    use crate::ffi::unboxer::unbox_any;
    use crate::{MerkleTree, mndiff_process};
    use crate::ffi::wrapped_types;
    use crate::ffi::wrapped_types::QuorumValidationData;

    const CHAIN_TYPE: ChainType = ChainType::TestNet;
    const BLOCK_HEIGHT: u32 = 122088;

    fn get_file_as_byte_vec(filename: &String) -> Vec<u8> {
        let mut f = fs::File::open(&filename).expect("no file found");
        let metadata = fs::metadata(&filename).expect("unable to read metadata");
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(&mut buffer).expect("buffer overflow");
        buffer
    }

    unsafe extern "C" fn block_height_lookup(_block_hash: *mut [u8; 32], _context: *const c_void) -> u32 {
        BLOCK_HEIGHT
    }
    unsafe extern "C" fn masternode_list_lookup(_block_hash: *mut [u8; 32], _context: *const c_void) -> *const wrapped_types::MasternodeList {
        null_mut()
    }
    unsafe extern "C" fn masternode_list_destroy(masternode_list: *const wrapped_types::MasternodeList) {

    }
    unsafe extern "C" fn use_insight_lookup(_hash: *mut [u8; 32], _context: *const c_void) {

    }
    unsafe extern "C" fn should_process_quorum_of_type(llmq_type: u8, _context: *const c_void) -> bool {
        llmq_type == match CHAIN_TYPE {
            ChainType::MainNet => LLMQType::Llmqtype400_60.into(),
            ChainType::TestNet => LLMQType::Llmqtype50_60.into(),
            ChainType::DevNet => LLMQType::Llmqtype10_60.into()
        }
    }
    unsafe extern "C" fn validate_quorum_callback(data: *mut QuorumValidationData, _context: *const c_void) -> bool {
        let result = unbox_any(data);
        let QuorumValidationData { items, count, commitment_hash, all_commitment_aggregated_signature, quorum_threshold_signature, quorum_public_key } = *result;
        println!("validate_quorum_callback: {:?}, {}, {:?}, {:?}, {:?}, {:?}", items, count, commitment_hash, all_commitment_aggregated_signature, quorum_threshold_signature, quorum_public_key);
        true
    }

    #[test]
    fn test_masternode_list_diff1() { // testMasternodeListDiff1
        let hex_string = "2cbcf83b62913d56f605c0e581a48872839428c92e5eb76cd7ad94bcaf0b0000e046367a5d29b16d7157f434d722c5f5bd8044950364d9c479332718160000000100000001f3382f83c5a0f4c3b7f4a85017e72be79aa2377ddce9ee9cfcec1c0d34fbf118010103000500010000000000000000000000000000000000000000000000000000000000000000ffffffff4b02401f0484ed195c08fabe6d6d6514aa332002bdcdf36ae5fe780027807d397a699b3a80ee7be2fa819650b48f01000000000000006ffffff5010000000d2f6e6f64655374726174756d2f000000000340c3609a010000001976a914cb594917ad4e5849688ec63f29a0f7f3badb5da688ac2485c78d010000001976a914870ca3662658f261952cbfa56969283ad1e84dc588ac1c3e990c000000001976a914870ca3662658f261952cbfa56969283ad1e84dc588ac00000000260100401f00007a823221095ab36f929350bf65f799d0a0f98114b2e4c30f289411e3c2da3505006242911ec289c2b1559009f988cbfa48a36f606c0aa37c4c6e6b536ab0a9d9eca178ef8076c76fd5eb17b5fb8f748bb04202f26efb7fba6840092acde00a00000000000000000000000000ffff3f21ee554e4e08b32c435d28b26ea4c42089edacadaf8016651931f22a8273feedc3f535592f2ea709aa3bf87f9e751073cf05b82aac3cf9e251ef3f1147f64a3af6c29319eb326385bf0128f89142530ec3f0832aba5d71d14ac1ff284cfb8c7ac1b59df6c5c41750cf71f1652828ff41af6e504bb2081165b27cf520ea3381f716bb1a105e261d00000000000000000000000000ffff5fb73511271710d647e3107b77440e2e9957092aeadbba86d02eb95ec23e490c023936bbd4eda6cf8850f98d01bddf4db0a405bc6a373f86e46dd739b18399d05b219de3217e751b201a01735b425b8ae3330507aa1fb4c5c679578bc15c814582d1e9c41cf0e11fa3cbd1dd41a1bf278f9d1b78622dca0d9533ab2dc65d71d7225975fadca9fa1f00000000000000000000000000ffff5fb73511271c0f9764003b7ede1d0d01f2cf16fc0f706f5394d2da1bacda404615c60d5bcb0b22a76776fd9be00f1d4a4a668ff3fa223cfbfddad5b5ad0644feead52e00560717eba6e901d79c543b826e0254c6c8aab06dd8b1445677df23d93f228798535d3030ea4ac2bbaf9ff7a4ffcf3931de9233fa8e151f187bf30235000e3b5fb102b01200000000000000000000000000ffff5fb735112711845e9bf2879d98ece4aa8b78ca074e32f968bd93bac973a1abafd61f900b70e7178b6352d830d0fecc2653d0f04a915189842a43e2868f4e5451c2051466a8d6b1bfc2bb01fe85d7358334177035e982db8388fd37b752325df17a53717b78e2b0c91b299f3554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff12ca34aa4e20157eff76f9632db9536c8af64a2283f3da7f91db86dacdfbf193ada958f69980e18d8d1f44d225fddcffc7176a941a26715baed3fbb275664b20a5c0f525568e609ab0c20120ba3f10dc821c8a929aeb9a32e98339fc2f7a3d64b705129777c9a39780a01e3554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff3f21ee554e26983c80e3e31fea6f3d56d54059e8c95a467285f33914182f1e274616cdbe2f1e1c6c0c7dce13710480ec4658208e9392fdf9ff7c06cbf660a2c93d466d5379492eb733420140befe806aca699fddfebe35a1edaede5bf3c33442cbd4a338db99ccb9f5717c6dbc9a92d48276c26702cee31a36b25c8002baaff59298c11e9457b42400000000000000000000000000ffff8c523b3327130008f22c7d80df8a9487d09ad2a93f7cdbff39b63467154fac0acb1035fb261f6fb6dc9c3eca70ab9f5939bc401d739c72c1b5e1df55233bf2b9548557e410481553de3200a01b70e913df11ee62bc4d21eeeea6a540fa5c2cf975952c728be8eed09623733554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff3432d0354e258dfa69a96f23bd77e72c1a00984bb0df5ce93a76ca1d20694e8ad20b1dfea530cb6ee0b964b78ebb2bc8bfac22f61647e19574f5e7b2fa793c90eaed6bda49d7559e95d30121958ba1693c76e70a81c354111cc48a50579587329978c563e2e5655991a2a35a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e380a9117edbb85963c1c5fdbcdcaf33483ee37676e8a34c3f8d298418df77bbdf16791821a75354f0a4f2114c090a4798c318a716eb1abea572d94e176aa2df977f73b05ab0141e985aec00b41aac2d42a5c2cca1fd333fb42dea7465930666df9179e341d2b5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e2b8fd425f945936b02a97c8807d20272742d351356bff653f9467ab29b4bdb6f19bdf863ad5a325f63a0080b9dd80037a9b619e80a88b324974c98c90f8c2289c3ca916580010106207e0dbdce8e18a97328eb9e2de99c87477cd0b2ad1b34b4231327fea08b3554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff3432d0354e298348757a2ed830f3a40f0e52ca4823f48c1ab5017dc424ee68c1e8fad27c0ab008bc974866db18bc76bc7d2bebb2997695ca25c4c132186aabbf5c7bfa331119a01969f9014172e5a561e36ae49358bc4c6c37ff688f54a05ae8842b496b86feb71f06b886bbaf9ff7a4ffcf3931de9233fa8e151f187bf30235000e3b5fb102b01200000000000000000000000000ffff6deb47384e1f8d1412ff39045ef39c2e19a75cb3ad986afc14c3139ed0a3392b41d471558676029a8137f95b0ba0e7315bf11c497f0fc8270f9d208c75006659cedd927f04ccf829242c018167aa267eb42b78d112b3600358ea7679328be8ceecec2cd68148985b6654405a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e3a07e472824512fd8004e7c81c3dff74c72f898c2a25f71246de316f6dbc976fe4e54d103e6276e457c415f53ba867e1d7ce2eeb1be671205ee68db332dd0f4871f549baf101a1d700ddb67ae80c1cb4fdb76dac6484dc1dc2741334ad5e48f78fd08713a13478ef8076c76fd5eb17b5fb8f748bb04202f26efb7fba6840092acde00a00000000000000000000000000ffff3432d0354e4d88a5857ea0eb8a5fe369bb672144867c4908300089472108afb9d54a70f7d6e4b339d01509dd9231da90b14cb401df2f007aec84e8af1b8f2da40a74b4d3beeaa8b47348012354b77c0f261f3d5b8424cbe67c2f27130f01c531732a08b8ae3f28aaa1b1fbb04a8e207d15ce5d20436e1caa792d46d9dffde499917d6b958f98102900000000000000000000000000ffffad3d1ee74a4496a9d730b5800ad10d2fb52b0067b5145d763b227fccb90f37f14f94afd9a9927776f9af8cfcd271f9ce9d06b97af01aad66a452e506399c18cf8ec93ee72ba9e09c5dab01e3845dbdaf3aac0f0f1997815ad9084c97f7d5788355a5d3ed2971f98dde1c2178ef8076c76fd5eb17b5fb8f748bb04202f26efb7fba6840092acde00a00000000000000000000000000ffff12ca34aa4e500a10b1fec64669c47086bc0f1d48ea6b37045f7e46c73c5ec41f7576653d7a6d7c79bd1215f16675bb31a59a7137241b99e1db7c082547cb66e0e84505f6a12e5a62733c0163cd3bf06404d78f80163afeb4b13e187dc1c1d04997ef04f1a2ecb3166dd00479521b08e5ad66c2fd6c2f514abe8416cd412dd2794d0f40d353fdd70500000000000000000000000000ffff2d20ed4c4e1f02a2e2673109a5e204f8a82baf628bb5f09a8dfc671859e84d2661cae03e6c6e198a037e968253e94cd099d07b98e94e0b3c7481f9b39efdcf96260c5e8b0f85ff3f646f0183ba23283a9b9dfda9cda5c3ee7e16881425506e976d60a39876a46ce82f38af5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e3c9446b87f833f500e114d024d50024278f22d773111e8e5601e05178005298e5fc2933e400e235c0a51417872f68cc20d773bddc2720f67dd88bfcc61a857d8d9b2d92aae0103df73261636cb60d11484684c25e652217aad6f7f07862c324964cc87b1a7f45a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e2f067ad7a999ad2dc7f41c735a3dff1d50068f0fc0fde50a7da1c472728ff33f9dd6b20385aaf3c34d9a259dcef975c48b9bd06acb04e2cf63daee7da0c65ce68715d5299b01045480c439ccaf9f38afff4a07e8a212735cdea7e7e8f2511c0883e2583e2b68ef6d852f7e1d547e18f881e4cb053d531b046c0750a8c620553524c81800000000000000000000000000ffff40c13ece4e1f05f2269374676476f00068b7cb168d124b7b780a92e8564e18edf45d77497abd9debf186ee98001a0c9a6dfccbab7a0af4aa6fd6b27d9649267b4ae48e3be5399c89d63a01c4842fe854e91a7b01fc1a1ec923e9f287da74f53a510e60b4b0bbb5433bf1dcdd41a1bf278f9d1b78622dca0d9533ab2dc65d71d7225975fadca9fa1f00000000000000000000000000ffff9f41e9344e1f15e97fb8029420a71f7125cbf963696c3fbf9636f6d2fa8997d35d37416e2c837182f2e7b7623498736253e5469eb894b2d4f9828fb06df1afb28683314ea5f84faf83f901a49e8534a2d427ef3a94d3ddaf2b05702e87e99d148739e949b64a7c1ebf695f78ef8076c76fd5eb17b5fb8f748bb04202f26efb7fba6840092acde00a00000000000000000000000000ffff22ff0f144e4f9155ae06f2e689f4fa68d5ff89e0d95feeacb431cce7065615d2de64095024e1b60bdfe740e5da5facf13cbbe9d06960265fa2c8a28b6abd1b22272a2cc52d2d8437317501c551d5597cc4f8ab6921af4f896ab68e6e71d15bfa8a1bec00769f6894157f075a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e3689fd3e2cc5690053c4252a2a95fccde944b141a3ac8e6b8c36c6b61e71d076f5cc4f9ba0f191d8051ea9b5c51cc5848059c75118444f9a31b03b4285c5dfb26da4f136b1012507422a27822ce0fafa7847828eced46309ff30968980ab12d74d8c751507306645fce7c379f7dc790472e00b2e4c9595c0a8932ec0102ac2e63fd00000000000000000000000000000ffffad3d1ee74a498bb67827af87431673e737c49312c5a16fd284daf1c4050e530b604ec4f85f217080503f978a6bec89d1ad4bca089c322583b4b7628ef1186853ff1166818d69d3aeaa420186d4f4152d96ff46c1f8ff948d11923899d2459f4656d5419682d8e16a41c7df3554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff3432d0354e218312e0ba7e4ace816595ade43d2293d70c3dce6b3e7e0ce9e99016f99177277bb42e6d3c2d687ab3e8bed13fb0d3489011dd36c51a435a18af6d4b28b7bc23e706953df401461dc135037403e79929e97099d82532e48cc3f877f8d243bda0673cc73198755a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3432d0354e2d8c8c7a5b96ed96dd5dbc40db042c301a9d70d5cb98ec073d41cc6a3c68d73ef0d6524cfa210ae1496a880e50fca3fcd15c5002882d6407c275f8850cafd70467a539a86301862677231ca31abd98e260e0678fe63d8580bf7a142a1afa68542aa7185409435a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e428077763a1a91d7595a05b06f805430cac72fc6737a0c0161624dadac33b21868d903d85bed5fc491be49f0653b8779bb40df4e10076edb905b93a9026c68d39aefb1883c0127847c25d2cde5ff46975adea87a4c6822f573ee66ac940d16ae205d9f6e88830c30771d54f702cdcc27c59ed99b19a36f0fae289fe666a5c51e43601900000000000000000000000000ffff5fb73392752f9426621a0df5cd8a4432c4050f39163a76ab39b2682aa3ea2064993265d66324be3d45ab22d5f9910c8ad09b96bbc952d8c76e6f482f2a7f933386eb007e514cbbd947fe0167964c4cdb2589996ddf706e1141b14c8bd3f293a3a9020dc6ece012a05827395a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e408e74c08f84a93b8831dd42e76537e8a17964123293de69c1cb24097035e0803822eac311434638fc73a5ad43739475425a3df63bd03dd223defa3c05f2de19deb9fc99aa01a9d8b58154cbc573203f183f012ed3c037f6ca26a8c2c05c85551d8382ef72565a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3432d0354e4910a2c0d103b24425b2359eac65b6997d2207c43700eed6371616a796c0a333e7401ad3a51b565a58bd7d0e604a0c80e04b4c9c61319479726a46b6dba6c1938ecf660c3101c9ab494d5c6fa05c87f689f9de3ef0da18b397a07361e436e5a2f2d4697056f0f87168d853c9859f79e28b08e7339456b5ea19a053c3a1edb0ddbe111500000000000000000000000000ffff5fb735112719865d6f26ed3f5309e4aed19583cf179bc779e21c967485f355b214ffb6ba461a01b575a9c62b3a02d08a37d01817af832e54ec3e1b5bb6fb8de073c9451760b2127c09a601ca485207dc1a501f9a694d7cd0f007846c40e7af8d148116c647923cdf7b9d996645fce7c379f7dc790472e00b2e4c9595c0a8932ec0102ac2e63fd00000000000000000000000000000ffffad3d1ee74a48101d302d6c69d9ecb9e13e755947f3af22f63ed4ecbf466ff64bd35c3d86bf2e4d8455ab736715d8f064c8c8e4d3c585b6a6ecf5eefa2ac295cc6103d1e42c7276e72172016a59f9b585d75f1b2cb43b7c0fb90a294fa45e0c1f7a432f82139a3ddfbecd3ed2b9c32684bce2de8afb2ba9bc9f7547d3a43fc88bf0a33ae2dddf6e3700000000000000000000000000ffff6c3dc02f4e1f0634f8b926631cb2b14c81720c6130b3f6f5429da1c9dc9c33918b2474b7ffff239caa9b59c7b1a782565052232d052a1bba3b56ededb76c1834f3b3e02c159aba4777a6014acaabcb7ee31149510138a0b11b087cc5a2ff0a24225bd52b52e1c2c58a113d5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff8c523b33271197e409f5889b8c033c412a939d2419824a2b0321e29c357a43bcc74644d1945c6a5fe7f8977edeb6210cb038039bc30f84a5d378f42cbf6b07b826f192cf691480e7070f01ea721d7420a9b58025894d08f9fecc73b7b87ed09277fa99dad5aa028ea357e176f5ce05c6c2a6de5d8a69c23a56f19dfc8f5f357c9457adf560f0f60b00000000000000000000000000ffffac3d1ee74a46983ca9ab507b3eb4e7b0d31ccef3f4553493ee5334116a3f79689f9b808a201ead332a26f7052fd17123cf142f96d85fc59f2dfb9d43f7570319c048b73a3b2e33f60778008a9a7d61d25db8904a3409468d81d49c3c190ec1f41371b036abf720e0a431fbd75782917293040170ddb8557a0c4dc1577d621f3269fe65409fddb40600000000000000000000000000ffff8c523b334e1f8b0c48578e5bfe77be25aec9e2745c8e699a6069411b3d9f90703a9f4dcce37bd62b511f3ff089422a7ba29d46e2b61675748f3aa899da653297380853319a3844e16e36018a3e98538662189dfebf2af3e9d22950256af8da8508d69ddbe4b247e846377c5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e4a86f43bc634d21567e456fea9ff6556e361d00da0ee46955244009847e492402c8dc82b4247330fab96ac9f0c538496b2cbea42ed3362203f066b29126deb6d1758cb16c3016bf850444b40d517c148f79bab13778e464815829a8a0cea7391c6d0c0e636bc8cd6c3d98dfa6e65171d7050963c7f9fa87ed1fadd04e028b49681492400000000000000000000000000ffff5fb7358027110077eb37d4559f880e21dbc3840a1a8ec8c32787fab07bd12e7fde1ad5f94ae95d6e4694f3533799d14e18c6832497428f95e1e0c681187eb1950ecdce67096ed6f5bd71008bc18fff3bc302051c51b545677173c459ae62e7cde27b31aa843193d870eb255a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e2e1393265406093dc6bda557446b9808a40f13896a683bd9801511c75752812f4a1ad4ecd3d9e9cf4c3afc62bb6cacdfe3867f01fcfb84d68804ff6f24f02b7e6d9a888e07018b3ba1e49dd244741546eb6f2a68c82d4990c55999bf385a070d27cc7534c4784e275f599b8886b49daa3285f5d4780d103b0475ce0123a2ae3dcb232f00000000000000000000000000ffffad3d1ee74a401828028671209b5196d2204d5bc3ce3ecd554dee9ff231883f04e67bea856fcae19d7a6154039140e9e3a6c6cf3fad4d3ee3c4a4a092ba275cdff0c03b912eb1d83c5e8c012cae9e9e4e356b719de38866f5a4b3727728a2a3d6a00a5f44075a015d0f9a0d5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e309179babbf6ca397dc089cbe29eaffb58ffc0afda1d6c7678ab3739d5b63c7f90428cbb4c4079823a3a71a25bf89f56b3c938bffbf5480f6f019e4aae7651450a8894b4bb010c67b8d2c4ab23820d89d82cb5f841b3b3734edda30f10aaa2c2bfe53a4c96645a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e4390a7a1f6b509e1f13c56262b8ebce0129cb751b16d8cd681634e62714553bc4dd773a88adf16184b9f951a9b8f0d1d54b85bcda0e2d5fe1e4b122a8d39e1ba860e451b61010dbde78360328a8ed44b5620aafd17dc85fdd76609b8dbb8e3e04964d02dd35bd912379d1d92a5c6922464c487848dd37fc15e7365086f30bf8995861500000000000000000000000000ffff12ca34aa4e1f16657cbdf9339c69126511f50b07253b69d7137f225d601d4674e2829091c4aade3afd83f9b1f3d8f0205b9e73603aafd3ef3a787dd137e66e96675ddf58b205bc32aee5018de9f50be6536ed7f22ecd23bb19447387dca77f2d67024a97ce394d0d538b163554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff22ff0f144e238e5e237e8bc750e5237f3c63bdd80034be58f6698ea1a696c29ff7f81cc251dabc5f925b65289e428461e2c74ba894ef0b5468254e500cb9881f73567c545b5ddfc51f50012dca894a2b5af0bf82abf0cfba978555f41a669278b6e91ca14c0593beaf220076f5ce05c6c2a6de5d8a69c23a56f19dfc8f5f357c9457adf560f0f60b00000000000000000000000000ffffad3d1ee74a4794b7723262031b6cd2e79b07f36a794d3e684c538a6f2418fff01c027fab1ca4663ab0b92670ee1797fa71d8676362a0ed8648ca7d2813a5bf93338e12d05db2e07d4d8c010ed18ce6df1e9a7a957852f65486a0eb17c70b8d51a24faf4a0a2e047100bb7b5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3432d0354e390bb7f50754a1fcd59d9d13bb7487060a7e6c9226af162de0d173b132e033e8da6e0f53eff5a1fe3a3d63e853217611a434762055a745abc52735a1fab5b7c509f873a64001ae4a298a414e8a8470e8c5b911a3c6f9200a806fa1dac65bf0e317c32ebcec8e84c09dfa353c83ce072c6a67d2081e80418db10e7dd8260b5acdd5f92400000000000000000000000000ffff5fb73392c34f92f5e861ac88ddd95e3829afc45f9358ea0973e19da8e42eabbfe8f2d9e5fa32204e7f1de5e20c7e45ee51ab262cf7dd70238a568f0ed035de6210a002f65fbd549ba8e101ee4ad05d852e40c676005d0d1830e45259b1c8b778e61d1b40a3758013f2988cd3730f336bd25c6a63868741dfced8b86873b3936c8590d890130d485300000000000000000000000000ffffb23ecbf94e1f12045d8b6c7f20036dac4c2f98b1d73c774ce4278b61412ac567b40f463ecfea23b3c291ed508cc7f2ff137cba1a5a028786b97f452773d464034d70fa600bcef8000c57018f248bfe318e4c9a889569491d58aa249a66a25716f09a1395e77229d1c14a94f56f0efbeaf436e98631e20bc7649afaa481cd62daee03f3af120daf2b00000000000000000000000000ffff8c523b3327159685ef9d056c2497dbdbe95e605f09e6b7fb0475051cdca625b53e3f761f20ce7353949e6e433f5cdb9cfca7ea0805699409bbb382053d51256b9622b11bf08e9919088a012f6dd2089625cd550f968418db3aa3aa7ec2c82b9936ea1b64ae3e49a9448ac45bda13c71a8effac4181b4f818da0cee9311c1e93a15787dfef825772300000000000000000000000000ffffad3d1ee74a458700add55a28ef22ec042a2f28e25fb4ef04b3024a7c56ad7eed4aebc736f312d18f355370dfb6a5fec9258f464b227e4d1eaa54b66e968265bdc5c88ce521e5608cb2fd01afbbb769dc2c57794ce684821642ada3253eba9dd86db233fc1a700fa728f474f5a34aa25efb245ea50e885e479abd9c71d32abc11a4e73882221b3f3700000000000000000000000000ffffad3d1ee74a41925d20af1a6d0ccd3890f0aead4a05a59be22e005b6d732f855311915b351a9153b2c83d84611b2c9958f806c93f7b5fa7ad30eadd503811c7f7dac3397e2544669e43d50190f28100e7b321ea2aa4e8eac6c21fa51757af4ed00fa835175f253da89a5b1b5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e3782dd9b3523f18f7cb1eb08ada48b7940eaf46aa9ed6cd1d79fe702d5d5689cbb552da3fa27a5f07efcbbb05cae2d5585dae0a3ff4070d41b6e0a13e0bdbd0af040d71c4101d023a6c0acc1b2847b50c93d64060c0d2a5a36778fb175c08d9567c537f7ffe95a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e321266fc3da4ce754a1c26d4f5656a5f9a3217cfdf70d2595c75b5e191041b3224c55a3542248a63a94b0ca059012aa7f66d61b96d1624f7a203d2b698b7ac446cd6cdbbbf01f000645cfc9a7d31dec37de64a6d6fefc0025e3882108240424650097cd4580e99c5d4cafe1844368f515708d3f8664ccf1f0c4644d9a08c3c80bab80c00000000000000000000000000ffff23a165234e1f995d3388b0289eccbaccfb505ebf86c8186507b5fe4b6f137ecdd7769340eda6cf44355b51493e35a722a40809cc42238bd206d0ac79c8f268227cc8308b27da055b239501f0308e1b7599190a2245435455e21d1dab86345f59f7ebd5931c93d96cd6723d3554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff22ff0f144e279470f54b992d3359b3a1deeb973a9ae4dbd0aa139b713448cd04547138f5855dca3065bcc61b1f1ff3390fe040f5833708c2ac3f1a099b745f17800a346a1c4bc920050001f15834be4b99fad17cf5857e8689241deb9f01ae1319d4e30f26fbc2a5a2a67218133ea3f6c6aa4a4c9ffb1927e68b99d93231412190deb9059a72662600000000000000000000000000ffff5fb733924e1f987d8b49e8aca918aead0d50b28fd0f61ed166f28b6365acef6a9aaee144a692f5b3cce00a40719917a042d16d1849b810c42881c057bf486a77dbb2926437523da4adb60111211081e2169c2294e986c881c1ba10a72acdfab62d9c7fed7d7aa1239194475a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e3e8260a3c57731ad90a95499e04802e094e4daad6a2ccd242761c1849342bd8cad744dcc5ebc3301fb9513c30ae82e8923e42fb7cd831ca6d386fbb8f945acf6eb146e2b0b01d19a8662b3614ccdffc199e6956eebce58855abe56cdd3a92d1de03a436a4d585a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e46873c0707fc70cc07e162664bc5bd0d61de2d8f2af0d0bf543c1411bf7a713360435923cfadaabc994de9156cabfd352ec33d99f91bbe4f3701c3684b0960c583a2f07b610111c3e585b5067a6d4cc6fd0690e66f8154d075598186be2bf04a39721ebcea118eebc867becf636c1871a8935b2a1a5e58f11c65dfcb36af325c30d41f00000000000000000000000000ffff5fb73511271414926e7ba179612df5cb1cc4ebbe311cfa9679e41f14ed7b35d12cc33d419073f013bf751be85f2b50e28910df33246349a36b7e56a5229ee2226219e8c4d1395626b92e0131af341643a35547531ab7f33f933b4bfac24661881e129b696516c8149e2ec3fc1d3f3a2e3f3459a2b5ffb6e67830f03ad7f3803bbf7ba5d32887a80c00000000000000000000000000ffff5fb73511271209f87f98c0ad49811131a31e94d875bb6c88f64226727a508094ea8e5f25f8f6cba8d2fb27f0f7e662233c565c1cf114eb3a40e2f4b4ac2cbcce12d71041f31b051d8684015235f5a493f0e83df8e088935262a1acb600073f8dd1e2215421a0f265a999455a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e44172cbd28e4bd100792c4455d761c523eacb2fedc49b6b20c67952e2ae5446d11931815254756a37d21d553131ce4a09e13bd7869b809fb31fb781f22948b498568289f0601b287610a92abd2251682e838175d2a742694dd64db25c22e794a3e0642dca689b90fe5000497348c6edb4c1c607ffb22b9ced8d6cf6551dca7c5380f3800000000000000000000000000ffffad3d1ee74a4307ffa44583c9908f4aaca8dd97990c56043e475723f90940ef5fd7d493152540f25f58fb8c965ee5e1be4f850a661476c1ad3af209f75deaeb9216fc8339fd48d376f9b001f22b5b0872698c28c6c5672aa0e62efacaa2664f9a79e49822fb61b7315ef1905a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e34071be08369f71e4b4b4e587ff64eeff402dba0e9c6dbecfe3b3f80bcd2bbbb433c01e42191eb0f5e3a95e56d8cb4a085a0d68a5ded90c01f23c7e86acc5deebfdc9fa4cd017480487c2ce567ebf6d3603dbae12a6015ce33e532dee9f0c7a4a9706a27d2f584c09dfa353c83ce072c6a67d2081e80418db10e7dd8260b5acdd5f92400000000000000000000000000ffff5fb73392ea5f95d5badff945693fd24158932b41e311e6fb3cca1e1e551eeed72cddba2e3b04abe86547a265fb7ee958875f9c33134db268e95c7ce631c89b56ffe1c3929423c1750d4b0174495e022ba898fe7753c55fb07ab876d087da453156b8585478d942adf1c47e5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3432d0354e4181d241a9f83b7dcd577fa215b1b2745cffff34d26290139bfbe30e8884b1e34fad596de7555797429029cda262f3c40625f547a7694aff98b19a7e9d4e344b4c1f7545840154ea82ebdb6b0eff568bf917ad5d0b8334ce294af9ec8268b37385d354714cb25a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e4c19c7b27ba7332cf4641137836b3a7ea78b9a53672a28d2ae5a4507dca234e5cf9a64406c98d96f120f3398075a23b0a645092ea3f00922937321d82aeb43f9df3c307dab01d509859a3a70d4f6c9c6430ba5a5c6ecd6f375d05dd1dd02cbbe22350d3b3bfab90fe5000497348c6edb4c1c607ffb22b9ced8d6cf6551dca7c5380f3800000000000000000000000000ffffad3d1ee74a4289e308c9d2d8a3cb35f9d7bb7220b1eca82c952b82111119670dacae18a509628c775287e4e796128cd6379b80dffd7d8d3433cb6b9a1a29fdf07613172bbfdab744889601d5673d1de3c45b85128b7e4e3f36706f7ca7ae424b377ea4693eded49e44c44a5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3432d0354e4587203a9163f2c3d296dbb8af5d7faec7b1fd204a8b1a5ec29a1c1c420d35dc88dc681b8752dd3fc5337dda715bfbc29aa85be0b3232261a9d146a884a53102964bcf37af01f592d892b27d10896c76bd13221870d9013e7c7a8b13c72e33a393c9c4e857dbf1652828ff41af6e504bb2081165b27cf520ea3381f716bb1a105e261d00000000000000000000000000ffff5fb7351127160dee44e338280a8e534c9e8bea9cb9d73163070d90d511e5c83859c384790e12da189e791404126eb2fe080593ad9a73ef664b6f476a958bcc490d4f170eb0ba926bfbdb01f52a17a6e5325b40485807bbe147bb3fda1aefa770505a71c71d7af3ba63ecd55a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e3384d8be6b408c129aabee53bac357bddd9eab338cd6cd96333a797d7fdeed36d7dbea5166b67b2cd160d46ac1bea832571aed89d64b9fb55bc49fa788c9b9c743c865505901358b1a766d3e8b71f90d3fe85231f2aa3c6504cbf2be7194d65f3a4517f459d93554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff12ca34aa4e24110e94442ed21e4bd3fe5b2e9726c3df4993fc61a10f1a5f37b6504b5d64b6e02e4f3177a6786876a1b5f756681000091a4b8c06d7f55bf4b036ce47ee75f675a715da4a0135372ba93701b380bd9df59538bdd426d2d995f257e90a63e88179ec87dd43f4f1652828ff41af6e504bb2081165b27cf520ea3381f716bb1a105e261d00000000000000000000000000ffff5fb7351127189809c680a8b7852279f00438526b2d940e65a0e746725adf2bf00ffc054ad2601b9011cf1edbd391426afd1b204d696f73f00a974aec5cfbcc3a34823f5736f0b7c8a4600156446d367c54618b42c90e54dfde45fbd07116dfa21425fa6db009a425bfcd3fc381e621c33b448e2d7bce9d631a2697a91f1916525b99937117daef2200000000000000000000000000ffff8c523b3327128a84f0696ae42026a72b89f066a4a55d3ee12545c672d0de9dfcd62ef63e8e0bd15d8febf1817ce2c76af812dbb9ab9af423ffc0afa8294451b9352ebc5a4ae45c6291bb01168147b72bd50c225608f2cd65ce90cf930c725b95b82e40e279b01b34e0a1c7dd41a1bf278f9d1b78622dca0d9533ab2dc65d71d7225975fadca9fa1f00000000000000000000000000ffff5fb73511271b802913fa3cc02a35fb8e1b26b644f8a2395078818f9bb3be8ad08fc8cb175f16c43e2b0aa2fc12a7f8dda3914946f702ce357534f97142aa704c8be0924950a7873b557501d6410cd8bb11cba290e20c9b183f0739cb630628994b66aa72c40b32ac38e2085a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3432d0354e3d10c0a1cc322597069f80ea22c04c4e5a442fff97ae4ec952a91eff9d8d9787760f20a227b1ada2025a6e9d74146e446752966d0882633324adcd18d1c4d9dd741e141e8e01b607b7d015fa271ccae5740685b3fea4be2b856da2e4f06838ed23038638873a84c09dfa353c83ce072c6a67d2081e80418db10e7dd8260b5acdd5f92400000000000000000000000000ffff5fb73392ea608851d988149766aaaafca285ded50de031ce42036033e3239f4f903abda26740ba235e22d26a693136a5ac27555f3de88c166622c0a0b8dcd8e5e70b630ce52a06b2e1ab01f92e1162ab0190dc924727897bf4d27eca18a3ebde01c67e2fb82c9205452f8decd2cf880d6b648eca3984400156eeed9443f809d23488d4b7ffbe621600000000000000000000000000ffff5fb73511271a8c01a1351c0f42892d6b68c106ba584f91dcc2869f384830c968688d09becfd0f7468e7ac7f02983724a6e95a887a1483cfbfddad5b5ad0644feead52e00560717eba6e9013987a05e9b1fb72cd13eb1ff70a20ea8fe5835e6f9ecacab48e0538c77b0a75a8eebc867becf636c1871a8935b2a1a5e58f11c65dfcb36af325c30d41f00000000000000000000000000ffff5fb735112713940c2271fbfbe83cd9dadaf03da32e840466cd4eb0e358749d5f22da2ca22610c6cdcb664b1c082b84cd4516d73ce5d56be7a3a831cac698f630a281ad80e7e343aced960159c38b8d6a0664411f92a6326e8ef0707ecf185405252854ddb477d89127a32d3554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffffad3d1ee74a4b932f6fc90c9dcaacdf9d836a2a7e60d090fe5e55b0b02f5a4f608a4b8235ba5aa7abc4e05f9387d1d942adc57c87f5b7c9fe0e7daab67759c331e39d4b9c05174e852f0701fa18d6f63ae9d791cd65d7cc1cc646bfcbb8706e2a6357364d5d58ea7696eaf65a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e3b18c60c196da02ae838b5cdddff0b84ffeaa5c72e2fae933d3a173914695f9d3f2f13a12567cf8a6445e1fd2472aad3a05f2dc45127038c872951c469596ae2f1b600437c01ba5136a927bdf3b74c279b277b3fb5dfcbf7773475cd770758c071ed882f186e5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e2c95758d8a2466f990857e8d1db8761463d8537f3c2cc59b94db7017e6d51f47075657c5ea8d1f801c9c71b34f3cf8b57bd1c5eea17a8f6a3f8d7155e76ae9581f6e518831015a5a77fa5422d4fd9fd45a991fd186106c6e0fa4cd151051067ab2a8624aff0b5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3432d0354e35919f8cf51ccd17f119db6f0bac1a37ad10e0b634c1a0cb76fab44b881fde5170247695275f0e7c10286fe58fff4a97ce2d3a1f4839a73715992e9c4ac586fad7eeab356a01ba26fb851daf4a571f2e58a0ee53443f99907fc0e5f3ec422820148d39d110525a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e4783bb6205d6d010166f99a1f6f671492210caceb92271b1ea21695f8831545f0f51a80beb06b054b558aca09e49e1b1d172d83737bf5ebc4ef9578ee806f253eec6c655e2011a3f5ec38a74ee8799405e7aad8bf265244f1e9d4226fb1099f6431a4a6170b478ef8076c76fd5eb17b5fb8f748bb04202f26efb7fba6840092acde00a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000227283c70c84ccd7853235d8e58496e23b0846b4009bdc261b20c6697097a0693ccc5d8e08342196ceee3c67b2f3f703af03638cc98eebc867becf636c1871a8935b2a1a5e58f11c65dfcb36af325c30d41f00000000000000000000000000ffff5fb7351127158a209b5083c2b601ea18a04f0e92ee5befecf765486deb9643dc3b3fd193080c2659bba166f3873364964d5e8f7e4b9344ba46c120c527424de6a08ce3ce2b7c9dc81083011baf6cc0d348c45bd7826fd085fa3a0bdcfa7441542bc94396e131da11e91dea3554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff12ca34aa4e2800381a3116667c251265178d35698c8a7c801a9765714c793d2dc03fbba5e9bcc9899a3b8fbbce3f07be22a36d0a4448fdc40895465e75cab40053a076bd3ea0a20c1194015bf7a008043994cdb799392bda7b5bbbd714ac2c9b2846a3ac7589be18ad357d5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e4b0872f496d7ca93e1e7e0e2aafac36c5a45e86b0780de1a8c5ec665343e7204538f116c5a4ace4fa9cec823ebb8f04af14dd387f5bcec1e45ee7dc221baae1095411402aa019c8dc7c74d6b97310a44dfd37ae62492c2a615bb660ede1e2adac449c52398b25a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff22ff0f144e3f1243421c72eb76a1d3058de901007b28211ca225466fd7fb046ecc0fbedb3d8ff59c144a46f720712e4525dae6bed584b600745efd3dc74652cfc4f9c6d99f9ccadf0350019c4e2fe4c34ab54bcdbf5fb8d3305e4e2c1499b789ef7bc90ec106ed62f236813554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffff3f21ee554e22902c12f4e752167465dfaa1cb88b45878c3602a0543bfb36be2ace7bd9725f7c4fd76446dabe9948f251adb808ac3ad42dff4926d431871ca8f78fb6b623c0e9ed113e7e01dc670576c36fd5a92e8e580ab4f898a228d6f9f66b19fcb3df185b66870577fb5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff12ca34aa4e4880f5458a1d7ae69cb8dbc6b7d69ac112e008bf34a985eb86cd973824023d8705cd02e2392f3c7682c3a9fac2a6c4ef48438f76f99f51d8f671f6872302cb9728e12cd7f1017db41143e8d6e3ca290a69a44797ee967e02f4297186fec08a8a0272f864369d3554b945bbff333cff1a0d4d95c848b52e558060fd7b2f2e37dadbfd1a00000000000000000000000000ffffad3d1ee74a4a862599b105fae8d252fef9707d02988e9f302ce6ffa7d1566908979816af6752e1470dab2f6bbed45ca65e64e4b74a3fec4308c6a9bb3109cf662b0f427cb183bc70f93e011d09e6c4660596a0cb7a714a044d41f208b514ee79147f9d73d5bcec839f4f9f83f4a966a456d795192b83bd2c866dbefee9dc9435dc3fdb42882e1c2c00000000000000000000000000ffff2d20d39b4e1f08e37b3fcba972fe0c2c0ea15f8285c8bfb262ad4d8a6741a530154f1abc4edd367a22abd0cb1934647f033913cca58aa2dbf2c2b6149412562f306841959b9cac234b73013de971af0ebc17861419d05f0250c7f4f073d444df1404914c5afe0afd65ffb45a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3432d0354e318cbd9ea64deb7cdff20eb3895473799d89c6cd11b5929934bcf5e28b04961be6398f4030f66ba12c13d1b749dcbfc4d361a091feea151cb4c9aff33d0c35b73226dc8a7f017d1ae31c1be20466c400285a293f88badafaad90fa8bc5b77d8da36631cf4e71677e5cc06119499c3f65458f202ed9ef3ce17d98ade4a6574fce0a653200000000000000000000000000ffff365b82aa4e1f08a37fd91db686b551ab91b86ab073c2c44e1d0bab4f99c1edfbc2b12abafd1e9a96715afd16173ab749db890276929ff1b9b7fc7bcfd49a904edeb77cafc68844897e91011dfbc526d3abe5490752532a2f305df97432c482c0404fd236f8bf8f0e4cc6555a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff3f21ee554e2a9625089978a7d330992669359de9168b481fa76ca0725ce4f55bd7561618109c9a8035f608723f68458ebeff4dba5eade27f552b67828b408a673ac6e30c11a054f25ba201bd27495194b6933f075412be4c301511eea1e0f75c1d8e3274cf89941a63f3b6dd41a1bf278f9d1b78622dca0d9533ab2dc65d71d7225975fadca9fa1f00000000000000000000000000ffff332650224e1f820710bf028cf0f81d0e8115f0654dffcdd83e598ddfdfd91bac653dbc534a3177844fe8c87e991727d581bcf775432e73787fcf2171e8644ea78cfcde314393bc66bdb0011f21e71597ffa891c91c1f24c4aa7925cef68f86ae9603c13b2b018e05651fae6cd6b4af357f054a9b9ae6257b20c9936034256118e06a3048c68f934c00000000000000000000000000ffff6deb45144e1f84175e1361b4f718341f496e3ad40644a99c292f184f7bf31ab2a711c6d3b63ad14fe4df227974fee5a8d4ba45fcf52118cd93dfee4f5f9aa9563d595f3fb4cf0f2cd8db00bfddb9d5eb05ffc2fa5573d09549f23ea9e1cd7ecb650f3eb16ea0f95cade5320c30771d54f702cdcc27c59ed99b19a36f0fae289fe666a5c51e43601900000000000000000000000000ffff5fb733929c3f1326ddac1044e0219dba7dccf6b43d1deed3e897717ca06757243b02516cfa67e24026f7a317cf575b40c10e7f6bf7f087da2642cf967c493f126137d4f15e9de36b97680100140100010016067cae80c66fcd79e3d7369c2c05cc331fdb23849f5f6b026f331800000032ffffffffffff0332ffffffffffff038b5797f440953049cfc28b8997f8029c8ccc2c5390f7dfc9621d70d62cd124a18ff35b0436ba341d3999789ecb80acbb4c82324321a47e9c54c3d7a3b5fa89d0ab2596ba0292c505c1652b1d0df1d4c386f63205f2514d79f9df488825466292f2891e6eb448c4b98d23641950f5d51e440b88c0d4ed729c8adc2153ba5f3000084cdc67672e8481f9dae91087962d3548b1fe6637d0818f8af92fc3078aafaa23fe1addc55792baf105309bdd83693886567104f9fb4f7f7842c2d2fd66b1cf9b56a7bfb73aa249577bae0b167ea0706d2d8aad39c474fac3583d142e475862052f9fcc787fa7b0e1f742e78b0f3af2eb03d0da35a9e779e874a2300748be8b1fb950ed420a431add47b8dee71522ab0100010d5fd66e2d6fe70070f26c97da9676e42030c9cb2609e940cc4e9a632b00000032ffffffffffff0332ffffffffffff03127e84f6dce16b9993e9e1fcdd1fd929439a0417054b6e9825b369686b46d1e8a1854ab3b59458a726e5ccca319974e501fe750a7ffb9ac53490259ed45c6f3461e4139d0e79914f1a507bc5f2c7c1b00c993b32d888cc9829dfad1ac3d28602d94a4a86f3948752c355bb2e22a0920adbe2e495e5a0f28bed7bd821b0084c71021318f4b1c67777fcf05be298b958491dc35ec8862c8f0a4a2432c692805d7537a2c4de235bc9a60ec023b19f8098f197b23e492b16786be48e9de07f27db7a3168586731fbe16869dfba161ee7d58319decd332aabe15f3a98a15e96a0ecc612ed28ed5989721782fa0bdd0805df641d5f183fe507837e231b084be31e0dcec22bf0770b59fda2951dd3fe0a0028830100011ad979c3e8c249280ccbd9893804a1cc2e80da531cfb2e4ca85b2a2bd901000032ffffffdeefff0332ffffffdeefff03891ef13657c4f57c58c7fd3feb07aac1dc3a48f9d6307f6cdafa09e80cb11bf509f885b41d4658fea42a2b9453d9cc172d2c738998926560e57fb1cd36964e963ee890b486c0ca689331479add7c9c5599c41918d81dc2a3fcbf11d3a37c31daf5c80beac4461186073b506bb56c33625b7303b782c8fae4c15fab7a0d81747d05becd9a50616593a917fb7f3dfd95c6eedce2d11a0bf1bdb40ea8561d70306f15c5b470119897840140a84297ad6ffe872ab0af8d7a02166075aac803764270dd8d324ac5aaa2908e554c8e0eccec5a1a2a2917937b9d8aa165dea12a3942cd16936d7d7d669956d1f7236fcc5b0e1b917eceb4f756e0594d598b223e52bb47f20a54c482a808a09090388ee37633990100011c9088e1db454f6301578a039bf44771a019117393012fa9d20133630000000032fffffeff3f7f0132fffffeff3f7f018fe597eb15a77d4369ac8be0cf2268644a0017c874b3c110ef94ad3df1829a31ac61bd9664cca4cf3405cdf008c660ec816a615c7539760b2f2ffee0fa522c2465ebfc4cda2822ec7aa5c7f020c13a97872c73566f9b2dd5aa6db7d5074579c020bdc422f68b7c85f1245a7782a5190e818315c635b7fcec7cf60374059515d70411b38ccde499b0e086f490337f8a8782021bfe72968aead0e5905525dd351ec6ff8c29d2cf2f307aac7471f25f5aca0f2e616daf78d72f15daee4bc3ccae2c7a934357f51c0f7669dddf6a3d50d89a4e53ed2c6a5a0b8e80edd922476a9151175106ec3612e0452fab85acc92a41e24fa5fc70e7693e57814a16183aee440f5244bf2393afc60dc6ec9e33c4c5329b0100011cc40a9169c62568dc56ca7c4c217defeedba53f74382684b59c3e6f0800000032ffff7ffeffff0332ffff7ffeffff0317efdd8de51ea29ded58de3a1fddff3bc8ef0f84734183969caec6c5aa7914be781ddc6024814caabef0ba8c26393a765233a109b0d2694bd08167e7a3ee2d70dc97059b9234abf51bd666499849907008b0d5f4ba9fad45049a74265e8d132d4649a1fbcbca489577ad7b579450ada9d53bf6605ff9bf9c917e5853e68ebe160f522cecfdcb5e8f560c7c17d8a1a0c921e667c46ebbe546db188d68e4e673e8d83525a9c0801b4e02b8b4be25bc121a02eb276724a91959eb02328ed066d7f716c01d2b80c95d12a02f93c2f3863544383179a3d65f47219d7fd67f40a91dd8107be89b3d8e593bdffe4dbefd965be2f34e16b4ae7f34033d4dc4c294646b09acf3df19884f81a3d8d433455ae9b14901000126d49bec6c7d1cdbf13949bd09e3317744e8efe9cda31185220d516c0000000032ffcbdffffeff0332ffcbdffffeff0398b95c5520c49148a37cc1f355ef4b3d6fb88b40c2c321f63fac75026128c87db7e098057e006c9df800676551907befd77832687348369ca5de677e0cd8cc01aa9c2f4f516767ae910816bccc19789f96b22817c4fe311c722919733ce244587186cbb674b4608412d7758d76d49f205d46421ce0d1eda65de202d0953888bd002d199b3f6a4968cd70ef1972310a515e67354c3a1904086e1990a4fb34798c304b01d1b173f45731fe2642b688fbad8a483c95e129c10388adc0feb9e5d8d34bf3ca1722efa2ec4b3f274a739d506a667c69dd00a46ac02c87567715cb159816af26d380ad85ee71cd741ff6e6a4845a0c75d5d13e46e7627e5beebb3173a1ad8b34f9d95a79588bc6d2777776ff4d01000134cb4f87e0427bc84210cd21cebc74353ee9bf9c32ebf8e214b7cde11700000032ffffffffffff0332ffffffffffff0394cc36d651ee5dfb39d53b71fe19cac355b63db5380c0120194d558b99d9fc3b2a8bdfda07aaf99e960f669ddc2a205f17bb44c61bd703188193c288c6dd72d90683a4a1cc280254b04708fa7f269795890f5162cbd5b6e7c33499f45e2cd140ed803b62cec9417b98c9f34e3552f50881fa0beae004fd0bf69fc2d7b5ea1cdd197a871f3d93a867d02ab7aa364fcf05f6d2e814ca66a76618c852df11085a24233e05d8207f9913d374475391c11d92917c608e9e1766b554854a4ed86b6ef9380277e8f30a6022282460bafcc21485b48b463838daedad96bb75b0884cff7708ba7cbb0f856fc37a6e8d67b72ca9f6a94b327609174ed1e6133952ac83ce4f1e421b5518ad5d141cbcd9634242e8d901000141d41a4b3ffe1aae316c357495b99a711b37436f2357d0cc25b7b3ec0800000032ffffffffffff0332ffffffffffff0399f4968fadbee9f34c921606012949dd0d573f494ba2d110b8107b797f04cb17bd81bbafa8277d256668777420e21b84e98e56fcb32d84365d962785e3901d47a204f76020abfab3de700f79c87c6dde94606c790182f70426af56b8470f0e919bc3e5bc68e85e01e68da26755234ce22883061cbd27e6ba71471d37152b99e905b38caa068833bd5c26c38229ba35e27903c28f3081e3f417ab578186bd5054aa695524707c530aea9a04190683be6412ed831436eb7e881f291fd8d67c102e0d273e726bcbd3cb862485a308570d2ef83cd35583ab88eee8255eef16d87e2f037c3b16ef8b9ca2523b45d1b282eb71cbb22619e41ac45f2e26671bd30cd2499ea8f171aef35ea808d56edc573aeb40010001679c4997606fcbeb78fc6dea5535a622fdfeb27aad1f0cd957ef5d752800000032ffffffffffff0332ffffffffffff03839df8ee86852cada6a498b618a06f24fa65a9b342cbf3613ddb570af0ead8def105bf003c220427353a9d417d5749ed64e453954dcc0c744fb3b5fd75c291d3e6f5771ab4ac6754dcb78d4bc58a730990cfc24e60e45157a16dfdb6dcf2945f92998d0fa2f3743b878885236692926a207a81e40ded33e0ce07ee96002376210ddc1b0ea397acc73a436f69f297ab03a6a923f3afca4eaf277cbeaa5bfeafaac507442a1b09b853d7e4239cf81380b50752bb15308ae784b48bf5394a2e8edf5829dc96c85aa519cc9ab4443ddd8c346ba74cd5139877ff752b7d3c05b8845e0ac95f3fc8f8cda08653d544b4f7455ed041cd005a687e73a45a9d71abd8fe91984178cff74b549bfaa20ed99c0916cc0100017d6aa5e197437547309c89d6da87721d1c38419e44a4f2c29abb98cf0b00000032ffffffffffff0332ffffffffffff03109e366d0e90d1a2f3a49a989df4f89396d2568eb5ca949c8c14f100a1a02dfb76a99fb2260ab30bd2f285b019d81418c9284ff1a5cf009f0b5b92053d44c2d3757c92d8e49bce2f8dab398f5564f7a30cbde83aec89ee9e655d1c4f5816c24a7fbd5134698257acbbbdbe2cac4cc003bc627d7dbfe07b9f082c882e0b22933303cdfd5d130d9da54f2f665306bbd5b5d58b9cf2e4053a412f299afda042d477a0b1a10b734e9759676b3019195f1063027d9ebdd76c3fdf516b29771f652d4055a883aa7e95a50eb5d6cfa04def3f2f2d57c27cb1e062eb063f9da0581bf1af0f65bea01977b2797e7764aed1349a354c49d80b8231fb201951c9df3a2507dccd066ed62aa6800d0e42e1ff250b85cb010001817519671ce9111fc37d0aafbd295ded77c5076492255361f0cf65101a00000032ffffffffffff0332ffffffffffff03901afc1e7456c20cc0337f1719a95bd81e748f48d654136818f5ed96a4cbc69cb6f4ab6cf84d8d931a80381ae1b4ab8ae8621b72234d1357d0a0a1f635d74009c0feafa7beae02133412e2871204ea98993910df8aa45883c3383c5a25b53cde5a4dde7f2c60bc6741d49ffb24501dc24f0ae5c293b3e3cc51e7a18c62377e7504131f263b6f592ef04c73ebf6c8b9d8f4408920284c510d99e4748d0e58e7f91c7570d31c491cd5c41786f6ca8fba6e97570b5c949b02753f257a957e53812050b5340440605add4ddf7d67893b7a35d199c712c7525d93e1c75279b47b0cb516146ccebd138b3a84fbd83b5fcc026e2b57f8ef2824246a6e6562624b150674d41e723a9ad23ee08e0a1f07d6562d8001000185611453d8c4e27360b689c62c18eaa0b95addb97db342246659187b0000000032ff7fffffbdff0332ff7fffffbdff0382234d2c2792368e972497afa4ed4f28df80efcf44cc76a019166bd3ec7f05f44eb5c53599f03305766b24a20a81391f86f42d79585f94c72a793b2ae969439a137ac39123b1cd72bf5b661d8707f598847d55ed1d11e37dbc564f6eafdafd5345134ffcd062cdf50c018b8c11920b34f872f74ac9d4d1d32f50e5e2730180c009be27c5e69461f29c1a0027fcbb3bd271111042872dec255413ac5dd9e1aaa6280d26bfb59f6936d84fd53f13f11b9d86d13e84e7c62e50b04fd6fc8952676abdaa499982a8764ea25363dd663a9307d9062b3232c6068c071e3ed5c34b0c1f0a0a6ff477be7f0e0efde09ec2c8b1b428fa5fe9579fe9eebf0ebeb1271d775d94deb5795be8bbad95d2001e51c422550100018d80561839648b844ade10b6e81069fa6c4bde6166dd59242be3487a0000000032ff7effffbebe0232ff7effffbebe0281d0717b893b557f54daacbd060bcffa2dc341175d0b89c7974dc57ef482ae27e10fb273eda534596993999950817cd4ed93bc215d15350bd7030be811cf1df2c114f6b34df9bd4095161af93608ed908d2bbb0b9c5b8626eb852ea0ff4f250919becc2d24653910fb8e11cf5573062f9f64c03a5031f1d462163ce98e8bf78a1470f7074a8e6fe23ccb53d73635ecd5ad71b26a938fc21638bcae7d272af9fa919f296a17e77191e3d4c708bc6e1b9a19e702ff84ff851312cceba1de528ee7ffe33647ae28ef895b35558512901394b430c804c7c42494a3312545606b55980480985494fa2c49f50c65d47570380f13c2851ce33d8584b64e8b659146d73267d821c78d09ba7caea3d03641f78c7a010001a148569984bf8f51d4a5f8e266744de7bc1e67897acb3ef6c16b98970900000032ffffffffffff0332ffffffffffff0316417bc1f47689eedcd10713c5b7754f75b97d538a55903f38e875227d711540c4876ca40b0f6f8cd2ad696a598031e50d53200d013f10415494b769218df8f33e0f9abced9ecd98a43b763de9c123f492fba8b76634ce3935a53a60c8e69100f3e66a80c3682fbae220260879c9ad67fc4d55e37cfda4b9f657e6cdacc6eae60ce5333712079938afdb6c82fb8a7f10d088dcfd34bba435cb76ac5d6710e328d037eeee8d1e042d63c2c53220da1b7c8d87f0bb7f55cbf3e0b1e90cf189e7394aa1ea2dff09d8358c83483663b356d9d68dbabab26561aed07d4669afdba7660c990b3d8954d34e40d7e2351115076fbc7c06fddd7c281296216097c1d77d4f92d28ade6032c67ed3d54b0ff2e147e2010001c1ab5b8d04affb2003750e0c06028f8b1eda3901ea54c9dc6111f2ee0a00000032ffffffffffff0332ffffffffffff0389f4c49dc5c223547163a793755687a6a25d1a8e9908c3cd040764fba24debd6de22f44b251fcd193ebe0e50284169e34260679dd6c451db091e6fd585de1afcb6fbb8e3e763fae2fc7a586969fe075618ab0bb240c29b88b69d984ce7f7700e58e333ae5f8110b439a45fd7ade36e9bf7c624c60931c0b4c889ebef943fe1bb08a9170919386c16a5108c0e35ba75048b7b0d2ecd6be9d9ff09cc8d2bee218c7a0345b35bd6148eb6844cc81008972d0acb1c0e79cee9062df3933a64179bf7d5f0e5c8049e9ab4e00c764da6b53bfa3351128023983dc6854f5ae1b3e0a4ba19dfb7c8e0cf0bac6b769e3d6fbbc734dbda2eded4dd0478d3c83e2bd886c3a0cf19b4515a5643fd72fce5027f777cdd010001d1fb1815f0a3188ad342380434eec3c28433ad3c3f1245de0056d01e5500000032ffffffffffff0332ffffffffffff0386802a52812933d4e0417793ced1f9f4e1c578ce4cd5e69e03d752550e30dc31228e38f1e32926ee5696d1ff6f319b36f6b13681a4a22670b01b29abb5e1303e7fd023b18a0823117af27ab24b0b4b7396c23addb7b3eb5a0a178be4b8ff0a6495e6c581bd54abedd0d7e57d3f871c39a1eeef02c8357536e01e616586a83e5304ae7b8d8a463d5fca20dd6ab7230398305dc145db3c414df218793b895ca4e4a93a1491aa7960cedba0a8d29cf254b487b14d095e03ce7be0b492fa0768b91654fd01006bdb6d1088bc3562bd704f70c7392539c2ced02c9b3375c86cd872ed0d89444483fc319bbc641ad2299b2a54ed2939734c16fff0c294dae280c05ec3785ff83eda3334fd54a357813b62c89a010001d4540de9899d1c4cf42c8e8cc6aa82d26b380f1eece4d3c98c0884690000000032fffff7fefffd0332fffff7fefffd0393b58ee38247653ad05f53cc90e86d7e93ea0a4f6b6a398f581ab122ddaf0bb7b4c28564a176545199eb72086411671cc5bd4205e47bb63d0f51746e83264859537b226b5bf3a909b41063b442e070cd867492fd032678317be0dad23e0e9ccb02c5340e1ca7ac0bb235c180059f1d1ee5464f996a0afdf98d78ab9c6b8f2bcf0051258101437c9a5b308c739b8c4fa453256ad860b46486fd1d3f5221bb33080f0e48d9f363e6140d1d41ecf6c0765b817f26fa6ecf210f9ff12389b5fadfc09a0b78da45d3cab0c7be29417b879a52158ee616e78c24e60368c9dd0ced616505f173ea8af4590e1a2fdcb2b365cb1f1aa59c3ed279af315d6cc10f759c4d602289c53789e8596278ebb6c778cb6e64010001e5ce43fe4705032af4a50bf3a5764a4f397d0d394f4b78fddf522bed0000000032dffbff9edfff0332dffbff9edfff03089ea9aaf700dfd7bc3ac551308d6a2e42a7988bc1b86b66786867594f364852a06c9cd38a31bb193cd40f5cfc3a91d2d7b5e991fc4039e9ea9fe063a4709c01f040a3a11fbfd3d17b5868af3eafb2898fd1d17c19a858f8db26cdcfd71e5bbe1a729e199f3fa489bee2334301357c2c6d89a8c29da071d065322b7739ae5915076ab217df1e2633aabb5d890b01f455a72f6cf5f8d5aba0e3be21af98bf36a2768d332701a3f1c24f6a01aaeafd3d9895a2fc5bbf3cade14e0e6c17def74846206bcc8b84fabe058f3190c9de14cfbb4b3f2232a28058c97d1b0ecaab458c1a0ca668d0d8cfa16a3358a760a43bad3d11ea28e337d21bc0691e98b41a6083316cceef5805bca00238afe2995d4c2f35010001eb34572e6b6c6596a89f11e3c7bd9f35c75987d7e3f25842a4b988a82e00000032ff7fffffffff0332ff7fffffffff031044eca68f0e6055355745555ea2338a9405290308add438931bcba80737f7ae3c6d2dfcfb0ca1472882e3a5c3dbfbf081221215915acec7ed705e3fe9f2135357c8bf074b0aa10b40964ac751f878c38ea28c44d80e62c5632e9161dd383c48d81095def4cffb8b5ed28d5a863801950d7ef589d6e39390ade9afebd4a6a4e506413904bdd8aeadede5aa385930b1fa3ae0527e8c576765a74b1c7ec95af77ccfe5d7183ca0c579d2bf57de438f6c32868e3b5bc07e172427f7151237ba60d8c505957239cecb59bd4dec034638be4e7dfd79a197ae651d0fa7e3a7eb82eb1f0db7e2cef5a092a19c0cf6e6c92d812b3df8b9e2b8f603054c3fde9b0f8f0e063179e1bc898a9720e3fc9fac77037ddb010001f180157efc2d6264291268be978288948e2407faa8482fc56699f90c0800000032fffff7ffffff0332fffff7ffffff030454e350d96050bc3438bd2cb4ba043f166e65f38faf2d8c38d1c70ce19dd7c505ff2d2145357a40b9c434052bf4943270eec32a5d77c70b62b64d1438032e28ed48cd18ddb515ffcf40be44092dc10d02116c58b65aee901fbc919c18ac2a73675dd894618684c26d394261b29423ba7443c10d3a1fd2a7c68489791daebab1138a94417b1bb36eb32cff0883bb4b5a9dee7b8925575b6272b81017a8e7fa309d54905a91cd10221e113f501f2e1a6887595105ef6f1aaaa09c468992a874016e2b9689dcf49b3723d7b370ffe2ea760c20644c7f9d34d04eb1193c850b938b1386cb0ff2074ab72d5b9e6282efdcd7121637943f200fc68e35d4f6aff660b0c446040611f76e20b578f3ba4f05b2b0";
        let verify_string_hashes = vec!(
        "2dca894a2b5af0bf82abf0cfba978555f41a669278b6e91ca14c0593beaf2200",
        "63cd3bf06404d78f80163afeb4b13e187dc1c1d04997ef04f1a2ecb3166dd004",
        "c551d5597cc4f8ab6921af4f896ab68e6e71d15bfa8a1bec00769f6894157f07",
        "d6410cd8bb11cba290e20c9b183f0739cb630628994b66aa72c40b32ac38e208",
        "5a5a77fa5422d4fd9fd45a991fd186106c6e0fa4cd151051067ab2a8624aff0b",
        "2cae9e9e4e356b719de38866f5a4b3727728a2a3d6a00a5f44075a015d0f9a0d",
        "f000645cfc9a7d31dec37de64a6d6fefc0025e3882108240424650097cd4580e",
        "11c3e585b5067a6d4cc6fd0690e66f8154d075598186be2bf04a39721ebcea11",
        "8de9f50be6536ed7f22ecd23bb19447387dca77f2d67024a97ce394d0d538b16",
        "90f28100e7b321ea2aa4e8eac6c21fa51757af4ed00fa835175f253da89a5b1b",
        "20ba3f10dc821c8a929aeb9a32e98339fc2f7a3d64b705129777c9a39780a01e",
        "e3845dbdaf3aac0f0f1997815ad9084c97f7d5788355a5d3ed2971f98dde1c21",
        "8bc18fff3bc302051c51b545677173c459ae62e7cde27b31aa843193d870eb25",
        "41e985aec00b41aac2d42a5c2cca1fd333fb42dea7465930666df9179e341d2b",
        "59c38b8d6a0664411f92a6326e8ef0707ecf185405252854ddb477d89127a32d",
        "2507422a27822ce0fafa7847828eced46309ff30968980ab12d74d8c75150730",
        "bfddb9d5eb05ffc2fa5573d09549f23ea9e1cd7ecb650f3eb16ea0f95cade532",
        "a1d700ddb67ae80c1cb4fdb76dac6484dc1dc2741334ad5e48f78fd08713a134",
        "67964c4cdb2589996ddf706e1141b14c8bd3f293a3a9020dc6ece012a0582739",
        "b607b7d015fa271ccae5740685b3fea4be2b856da2e4f06838ed23038638873a",
        "4acaabcb7ee31149510138a0b11b087cc5a2ff0a24225bd52b52e1c2c58a113d",
        "f0308e1b7599190a2245435455e21d1dab86345f59f7ebd5931c93d96cd6723d",
        "6a59f9b585d75f1b2cb43b7c0fb90a294fa45e0c1f7a432f82139a3ddfbecd3e",
        "56446d367c54618b42c90e54dfde45fbd07116dfa21425fa6db009a425bfcd3f",
        "8167aa267eb42b78d112b3600358ea7679328be8ceecec2cd68148985b665440",
        "862677231ca31abd98e260e0678fe63d8580bf7a142a1afa68542aa718540943",
        "5235f5a493f0e83df8e088935262a1acb600073f8dd1e2215421a0f265a99945",
        "11211081e2169c2294e986c881c1ba10a72acdfab62d9c7fed7d7aa123919447",
        "d5673d1de3c45b85128b7e4e3f36706f7ca7ae424b377ea4693eded49e44c44a",
        "ba26fb851daf4a571f2e58a0ee53443f99907fc0e5f3ec422820148d39d11052",
        "1dfbc526d3abe5490752532a2f305df97432c482c0404fd236f8bf8f0e4cc655",
        "a9d8b58154cbc573203f183f012ed3c037f6ca26a8c2c05c85551d8382ef7256",
        "d19a8662b3614ccdffc199e6956eebce58855abe56cdd3a92d1de03a436a4d58",
        "3987a05e9b1fb72cd13eb1ff70a20ea8fe5835e6f9ecacab48e0538c77b0a75a",
        "0dbde78360328a8ed44b5620aafd17dc85fdd76609b8dbb8e3e04964d02dd35b",
        "a49e8534a2d427ef3a94d3ddaf2b05702e87e99d148739e949b64a7c1ebf695f",
        "0c67b8d2c4ab23820d89d82cb5f841b3b3734edda30f10aaa2c2bfe53a4c9664",
        "045480c439ccaf9f38afff4a07e8a212735cdea7e7e8f2511c0883e2583e2b68",
        "ba5136a927bdf3b74c279b277b3fb5dfcbf7773475cd770758c071ed882f186e",
        "7d1ae31c1be20466c400285a293f88badafaad90fa8bc5b77d8da36631cf4e71",
        "28f89142530ec3f0832aba5d71d14ac1ff284cfb8c7ac1b59df6c5c41750cf71",
        "f15834be4b99fad17cf5857e8689241deb9f01ae1319d4e30f26fbc2a5a2a672",
        "a01b70e913df11ee62bc4d21eeeea6a540fa5c2cf975952c728be8eed0962373",
        "afbbb769dc2c57794ce684821642ada3253eba9dd86db233fc1a700fa728f474",
        "461dc135037403e79929e97099d82532e48cc3f877f8d243bda0673cc7319875",
        "8b3ba1e49dd244741546eb6f2a68c82d4990c55999bf385a070d27cc7534c478",
        "0ed18ce6df1e9a7a957852f65486a0eb17c70b8d51a24faf4a0a2e047100bb7b",
        "8a3e98538662189dfebf2af3e9d22950256af8da8508d69ddbe4b247e846377c",
        "40befe806aca699fddfebe35a1edaede5bf3c33442cbd4a338db99ccb9f5717c",
        "5bf7a008043994cdb799392bda7b5bbbd714ac2c9b2846a3ac7589be18ad357d",
        "74495e022ba898fe7753c55fb07ab876d087da453156b8585478d942adf1c47e",
        "9c4e2fe4c34ab54bcdbf5fb8d3305e4e2c1499b789ef7bc90ec106ed62f23681",
        "27847c25d2cde5ff46975adea87a4c6822f573ee66ac940d16ae205d9f6e8883",
        "4172e5a561e36ae49358bc4c6c37ff688f54a05ae8842b496b86feb71f06b886",
        "b287610a92abd2251682e838175d2a742694dd64db25c22e794a3e0642dca689",
        "0106207e0dbdce8e18a97328eb9e2de99c87477cd0b2ad1b34b4231327fea08b",
        "ee4ad05d852e40c676005d0d1830e45259b1c8b778e61d1b40a3758013f2988c",
        "f92e1162ab0190dc924727897bf4d27eca18a3ebde01c67e2fb82c9205452f8d",
        "ae4a298a414e8a8470e8c5b911a3c6f9200a806fa1dac65bf0e317c32ebcec8e",
        "f22b5b0872698c28c6c5672aa0e62efacaa2664f9a79e49822fb61b7315ef190",
        "8f248bfe318e4c9a889569491d58aa249a66a25716f09a1395e77229d1c14a94",
        "ca485207dc1a501f9a694d7cd0f007846c40e7af8d148116c647923cdf7b9d99",
        "7db41143e8d6e3ca290a69a44797ee967e02f4297186fec08a8a0272f864369d",
        "fe85d7358334177035e982db8388fd37b752325df17a53717b78e2b0c91b299f",
        "1d09e6c4660596a0cb7a714a044d41f208b514ee79147f9d73d5bcec839f4f9f",
        "42911ec289c2b1559009f988cbfa48a36f606c0aa37c4c6e6b536ab0a9d9eca1",
        "21958ba1693c76e70a81c354111cc48a50579587329978c563e2e5655991a2a3",
        "1f21e71597ffa891c91c1f24c4aa7925cef68f86ae9603c13b2b018e05651fae",
        "83ba23283a9b9dfda9cda5c3ee7e16881425506e976d60a39876a46ce82f38af",
        "54ea82ebdb6b0eff568bf917ad5d0b8334ce294af9ec8268b37385d354714cb2",
        "9c8dc7c74d6b97310a44dfd37ae62492c2a615bb660ede1e2adac449c52398b2",
        "1a3f5ec38a74ee8799405e7aad8bf265244f1e9d4226fb1099f6431a4a6170b4",
        "3de971af0ebc17861419d05f0250c7f4f073d444df1404914c5afe0afd65ffb4",
        "bd27495194b6933f075412be4c301511eea1e0f75c1d8e3274cf89941a63f3b6",
        "6bf850444b40d517c148f79bab13778e464815829a8a0cea7391c6d0c0e636bc",
        "d79c543b826e0254c6c8aab06dd8b1445677df23d93f228798535d3030ea4ac2",
        "31af341643a35547531ab7f33f933b4bfac24661881e129b696516c8149e2ec3",
        "2f6dd2089625cd550f968418db3aa3aa7ec2c82b9936ea1b64ae3e49a9448ac4",
        "168147b72bd50c225608f2cd65ce90cf930c725b95b82e40e279b01b34e0a1c7",
        "9bdc261b20c6697097a0693ccc5d8e08342196ceee3c67b2f3f703af03638cc9",
        "735b425b8ae3330507aa1fb4c5c679578bc15c814582d1e9c41cf0e11fa3cbd1",
        "f52a17a6e5325b40485807bbe147bb3fda1aefa770505a71c71d7af3ba63ecd5",
        "358b1a766d3e8b71f90d3fe85231f2aa3c6504cbf2be7194d65f3a4517f459d9",
        "f592d892b27d10896c76bd13221870d9013e7c7a8b13c72e33a393c9c4e857db",
        "c4842fe854e91a7b01fc1a1ec923e9f287da74f53a510e60b4b0bbb5433bf1dc",
        "86d4f4152d96ff46c1f8ff948d11923899d2459f4656d5419682d8e16a41c7df",
        "ea721d7420a9b58025894d08f9fecc73b7b87ed09277fa99dad5aa028ea357e1",
        "d023a6c0acc1b2847b50c93d64060c0d2a5a36778fb175c08d9567c537f7ffe9",
        "1baf6cc0d348c45bd7826fd085fa3a0bdcfa7441542bc94396e131da11e91dea",
        "c9ab494d5c6fa05c87f689f9de3ef0da18b397a07361e436e5a2f2d4697056f0",
        "35372ba93701b380bd9df59538bdd426d2d995f257e90a63e88179ec87dd43f4",
        "03df73261636cb60d11484684c25e652217aad6f7f07862c324964cc87b1a7f4",
        "7480487c2ce567ebf6d3603dbae12a6015ce33e532dee9f0c7a4a9706a27d2f5",
        "fa18d6f63ae9d791cd65d7cc1cc646bfcbb8706e2a6357364d5d58ea7696eaf6",
        "d509859a3a70d4f6c9c6430ba5a5c6ecd6f375d05dd1dd02cbbe22350d3b3bfa",
        "8a9a7d61d25db8904a3409468d81d49c3c190ec1f41371b036abf720e0a431fb",
        "dc670576c36fd5a92e8e580ab4f898a228d6f9f66b19fcb3df185b66870577fb",
        "2354b77c0f261f3d5b8424cbe67c2f27130f01c531732a08b8ae3f28aaa1b1fb",
        );
        let verify_string_smle_hashes = vec!(
        "f75d5984c7136a0c52f0b1c6aa87d4203907a0d62d1b6ae06ab6eaa021b44199",
        "b1686aa4a68bb56fa915dc2249394661cfabc3b0240351e49593ee7079c74bcc",
        "4ed757fa84f0a1d279373e2841196186c9023c90948371c7ccab25554824efe8",
        "65b884169a46354b3ec79848fec5a6a8eef5302748a6244fa56e34a2c06734f6",
        "40e747e109b975101954113683a0a5b25bafecea3e98f27232c60b47db50e6fc",
        "7ca916b4b9fb3549f6982cde58b992a258c372fea0d6ce69043f99b48315b73f",
        "0c880508bc97150e4b5ed8232a11c27df401496f5afaace0ce2256e462adfe76",
        "df88b8c121af50e64b998be0226301d6e6108e0afb1573e4fb2cf7e00c48e9a0",
        "20e27d2e035002641212e3db6be3750b4d0cab6c2faa9a9561778ece0cd6f1af",
        "4babd7eeae3327a402132abd7b9ffd97225a64b2c4206f6a08cdd2a7dd6ba3f6",
        "ef99e5a033ec235ba80c71bf4bc44945c4344a2f1712aeb11c1be7d9c07ae606",
        "b95013b285a944bf810d6cb23cc613b0197604e74db0fbf2074a0c72df4dc95f",
        "70c9dc3e6e2421d7eeb8be379284f470b43592298aa6cfcc5d2a538e46512f4c",
        "9c5da1bacfc6350e3329a2cfac76ec78d2351546fdc224971efd79e412949375",
        "41373b50a6ce69368ea7cd4ad13d663f777653d8ac69a1bc7a305a8b99fee36c",
        "cbcb8eb2d8cc5b609923114546b63c54f7a7ecb66918a143914f01d23d145170",
        "43471bd379d1cd1933d88ef2842a203de9334b0c02976025256bf1fd7df99415",
        "5a2952f5b21b6c251aaf51d1784c355eae94bac540fc5fcda2b41966cde84056",
        "fb6b938139b4c2a48ad5bebe9caf5f2083325f6c7413c194d2c0870d0f5a1d9f",
        "d9e7936959cb47f52d36fa5b81bb16ac8bcdf99f26e492184c59678d2a62e7f1",
        "dbd7d7bf750a0b7b35d4012f2d676eadc7212fd5d66f474069ebdcd50bd25079",
        "a1196d35e273c9bdd334b2215c56e401be44f9895026cb52965b6870f1e3b98d",
        "3d138eeaa5f63b564835d4f0348ea57d97486cb835a084be919dbaf6ffa6ecb6",
        "73b309fc6c9799bae0358ef888b068a7aa20eb1de2383bd7d3d9c030c1e8b8fa",
        "914d162c56ea339b19901c4a44cd88bcffc683caabe54a353789c09cea9e9f56",
        "3bde2f97319f1a588f095071b27da7196b339d57860c297ad0b3c15f2913440f",
        "01e4554ca14ae334274026dc6f7f586e456516a54dce16805bd3bb7df4602be2",
        "0fa4ba7423d9e6a4b0a9521870138cd613ba91d155645194d81017ac5743a5cd",
        "4a3201d6c220ad5d838e8ed149caf4402bd89f3c1ad6e0957b1354cc275affd5",
        "ad86aa32d2fdc821ed9ab248392e3e49ed0d28cb96014d2d35eeb9a7e3013ab0",
        "92c3933a79ed2c4f90c50ae89fdf2fca5fecc111b490e371889ba8e15efc41ea",
        "919d40eff132a9ed31853c7384e4a7456b3bae5ba17f4f3bb1f402a5189adb4c",
        "df88465189c18e15945cf37df808ca494a7b2ecbac36e4a1eb8a67802d14fa65",
        "4b168b226030c1cd953b94cbeee4a26b761a2e39b370626abb8901632cb08db7",
        "0cf472d8c3fb23b4456c1d76a2fa0f899457ea6db926ef29fa6e1ccb32a74fd6",
        "c90ea0ed583bc95f3fb8e2f45319aeca31f0d4e272e53869ddbc68aac662bdc4",
        "3a5ce3c8c32c209cdf12a234b42aa8e38bf2bd041650a5043e88a3c27ddbd403",
        "b178ceed4a30285679797155aa6fdbda31663a4d9e8bcba981c332bdeac022e6",
        "0e4751a43e8c9d209b16cecf135d35c6003181bd6e2c6a97c04a532fe68ceac2",
        "598922001099fa40502f3ca4b7e7456ea48244d68be5fabc8ba1b29b62d7325c",
        "370fde22aaed5e2b0da27043ac2ff0608b52b29780e41ff021061030443ae20d",
        "ab75bb47a1437e1a893b24b6a682f2845aa5a97e8b533e31d5da252c41f723f0",
        "71cf8fa5a73ea9ddc7e09cc1a70393a8e85a810976018a53b52d147d686fd367",
        "93edc13c4df24a0dc3b3e265a086bd8f69917ab4613c7259654f4e67e140951b",
        "1e04ba43bdb86cefd34b00d61ca3ad80e652ce30d91298203299c7e255e53ac9",
        "be9e5dd529fcb66b8a72dd40155bf06e4cd2bcd389913f63290c67715e3ba2fc",
        "642fd11e9eb7727fee1aa908706d30c09be7a8bddc5fafb49a2431e9625d8d7e",
        "626908ecc1d017449f0bcc393baf2c350d9f484be004789a547e9cf94a7ec6fc",
        "76d14be3721e4534db2f00a0a2cd486603a043eb0bf155566987146418e0a637",
        "1c3b17ef431517a26f2c2d1494bad4b06686452922c441de7c0095eedc5a93a1",
        "134126313cbf4aef0a65f15b64d8c058975196ab3dacf30a361386f210b157d4",
        "77d6b60432de60e77a6921bc58884e0f52c1e678a23863708fbf6697c9b0d19e",
        "f6d11e1ebf291bf4ac6e574c40f86a83eb12a4b73a354276b3006fa216803ea9",
        "d823378bc63d2a44b24f04dc750ddfc72a5793211121e58e46ef178c9f373d64",
        "15cabfebd4c657b5ff2bb4c6bfb50d562935bcecf75bd70f4a981914344e8b09",
        "c2ec1153a273d5adfff01bfd08f595baa0e8126481f2617e4ea37011c55216e1",
        "c4d1a2e7bdf9ea419856d29d6c19a2dbca44ca5b337f48eb784c868f08719083",
        "10bb48868e7a763e31f2ab1a5fd24d7adfb6e19308dbccc4b83183024466b11e",
        "3c01accf8fa4c60c3ff14d2c5874f6d2672cec4efb726b80e80415f7bf03175a",
        "c61cfbeb41b91e6e99126fa76c5d8a8376088d99385347be18f2ee750614e566",
        "f23c8b054a28f4f54a43e14269b117966d51c8066b4ee69bb5368902a24950c5",
        "bab1188c2037ded50d38f4cb875c6c61bacd19e4ddcdafe4eb86c0d4c4a6827b",
        "0038e73b99e343ca975dce01ab1c63009b68994acf15a88c94f5950967053237",
        "6e3276e3745af81f229a19797720f4c6f4b96654339cab77e5cf1b54693ad368",
        "1cef6569dd095177f0cd12ef2c3d8f48081429eabc8f497185b90c73206a172a",
        "2f3cd563c4ac12aa3ebb9a89f242b69ce2eff34a54ec83e75446edbff03321e0",
        "84edf5320c8524531c3cc843996696026bea406c326990ffa265e30b29aae256",
        "64b86b8b48f4d9f884368d79520a0adcc239687126bf245400ff81a4485e4a7f",
        "ea4c93192dc54a4a862a992a2a7a47e77ed8fd4c454ea92de663b77ac5183e6f",
        "b98bb14865b11f9b2f3e0d54cbca6dcec2a1dcb4d256e5e0f1951b434b9196e1",
        "264fc627722365937d903c2e358b93fb33d3cd2a5f06cc8b8bfe7a400bbd3375",
        "e7871a69194eb1837aabc4ff5460d357c17f5dd02bc5248195ab55a43efe3eb7",
        "4acb694ee81a889b3e1e0e0a0f56a2ee2162a17cb3df6ffa6484520dbaa39b87",
        "370c132e0170da64bebd0b738a63797bc2515c350712ab93c63b4ebf10ae5de3",
        "1f4ab767f64d321f61d0a0995faa3096bf54742d62efe594aeabba6dbfc7e830",
        "c2e326a9cf0eef8f71f88607caf6de05470280bb39d37d1001293da38c51f219",
        "f75ab125a732fdc03d3310fd86467c5fe0ec2b105bb78783ebf1a24c2beb57a1",
        "e184bb5593f302b5f7463520d3ab13db94788d9000e774b1560299672914d511",
        "65b25e426b55fb2ca44c95b2738d775b97304c0af18745454f92d59f96512e80",
        "1d02719acd012fef9c6734cfce121fb1e2b3bb2c135994699e6b99568ee0d1be",
        "c5147cf07943ab5e7797947b5702c8b80b55bf1e6eaf6a00e5311c471fe32dce",
        "35dcae6d88a079188b8da96789104dced067c57a82a22c7e10a606d7b4bda1dd",
        "4a7aa2a5408002b4742223809565eef9aaa6e0681322a031f859177813167c9f",
        "2606fc8205ad4b1a89f8cd4b1568ae029f8fd2a839afe7456a1f95c890d91a6e",
        "e969ef9630cba22800158836c3ef72c02b4580d02fb7c4a8afa2f5b85340fd8c",
        "40b2b42058667f969bcbc2015897494baa99440aafe374e75b07b9234ae0f317",
        "f7a0c9825ba4353de2d6190ed0164d6b172ba1a778984b15a28e50476d8b61c5",
        "2d3c332768ef0d89653773b5114a6bc14abe351137b6942d4ec3cfd176e52a28",
        "ebb1f65a8af9af3f4fdb258c5acd38c7e9ff656836c83670710faade59e6bcbe",
        "7c7339dc4ddf10eca4abe00251e65859396914c56030d4faff1b2e91c295f4fe",
        "5a832be116f16a700054ed6761052b03c40d74bcae666c2b9b1cc3a019a36e21",
        "ba53501b35ff90f93d6cf99462ef96211be215362eb9f9b6c65464531e35d0fa",
        "4bdc967d3f0600f6cee7874ab9064e0298cd9a0911fa48e7f46b7bf920ab12f0",
        "6c21634ede6b6b0bc7539349a3edf884137a8d418fd73a00d7d449bbaa40e833",
        "7096d138cfaad0855a052c603f5666cd1bae259552c2e97237fe3def4dfd05b0",
        "0ac44988afcdb876cee272a59fe30546d800fd3751e780a3c1f9bd2fe0370835",
        "02b85de6d2827d76c97b76a6c8aacc3547561a79b52bdc40a99b3425ea77d9f9",
        "6d1c7cec21aad2b93d9823cae3b39e66636a34daa033008d964d660301508684",
        );
        perform_mnlist_diff_test_for_message(
            hex_string,
            1,
            verify_string_hashes,
            verify_string_smle_hashes,
            block_height_lookup,
            ChainType::TestNet
        );
    }

    #[test]
    fn test_masternode_list_diff2() {
        let hex_string = "dd44f406dd871809340bbc4ea57704ba30b071e56f7a7cda9050520300000000da26ba5865c19fc355905792aa93c4d712cfa6835aad4bc2c145ae010000000002000000023b5e149504fbfc1918cbc4aaab44facbbda04e6215ca285967d1db80df2784437b448b5158f30edea16a64fb7e26a46f4d276caa2eaaa76c9ed930028269fa3e010303000500010000000000000000000000000000000000000000000000000000000000000000ffffffff4c03ecc70104e482015d08fabe6d6db432313a00009a9659a973dcf54d002a33e432871286fae7589ce7284257998e01000000000000005800001e8d0000000d2f6e6f64655374726174756d2f000000000200240e43000000001976a914b7ce0ea9ce2010f58ba4aaa6caa76671c438e89088acf6230e43000000001976a9148ef443a84e2b900681658633bcfb74fc9907fb1388ac00000000460200ecc70100cde35283e3b9b7d40cd29c0cccd486e0688fc1d3b9a3bf3da5cfaf652969784fb9e5ab20f3848e99574228e79c58e7fa86128732e4332a004df5dc73a959969a038687a74d07ec7ac88b0c33086840a14b5699cba97608958ae325e57ae5c45252cba39456575e98db070d408313dda1fad6a16af765eb729506dec902134e7538df4e5b556d2d354fae3bfc4f396825011d42c2bb1fb99baa5b31924fb3840f36092354b77c0f261f3d5b8424cbe67c2f27130f01c531732a08b8ae3f28aaa1b1fbb04a8e207d15ce5d20436e1caa792d46d9dffde499917d6b958f98102900000000000000000000000000ffffad3d1ee74a4496a9d730b5800ad10d2fb52b0067b5145d763b227fccb90f37f14f94afd9a9927776f9af8cfcd271f9ce9d06b97af01aad66a452e506399c18cf8ec93ee72ba9e09c5dab0123cfd922f1991606944a5e178e75e3cca63c3fb7e94720b4e4649403d9116c0c0a60ce718c6a479feb17b13a16d18245222a6a8699ff8b9a2cbed6fe07000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000bd185938a5bb256c9aa0e6a2d86d6a687600c9c0004431d5c73ecb095daa699363d0a19e5b6e549414e6608cecd1188dbf551a777f87642a37e81f3d63e25a4426e06087954c872fde1894ea3c937517060000000000000000000000000000ffff2f4b449a4e1f842b8e5b5cc0841de193f440d5fa3e0b4a34df7fffa798fb8c3df46fa31187162cc3b3ecf929689ae35e04cbac6e069ff0a5dd8f973cec456a2423da87b5f1aa97f282e00049e7c0077dc7f325ae9402913e681d3a3433e407e8e3f7670d1e0e1f56b8e37f9fc11712d2dc9bbc1086769db77a550a8315ff64ac5e20dfbfdabe070000000000000000000000000000ffff0567885a4e2582c635ec296af1d424586d4a77d10159fae30591e6f0d7e64060ea613dd8ee5392f9e0f2c14156147ee849b320e76b68c74055edbaadc7948fae10912f87eed3652a6f5c014acaabcb7ee31149510138a0b11b087cc5a2ff0a24225bd52b52e1c2c58a113d5a4a633d79445e9acfbc878f263e03b0a3c930091f7383b5467eba7b4c00000000000000000000000000ffff8c523b33271197e409f5889b8c033c412a939d2419824a2b0321e29c357a43bcc74644d1945c6a5fe7f8977edeb6210cb038039bc30f84a5d378f42cbf6b07b826f192cf691480e7070f0093f882118caffcec59e7305e5141ced538880a37dbd68e075102f9dc0f51c22bc722194540a1b092c3554f6ebd9faf2dbd04acc8be221f9e3596311b000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b017f68e82b1206e7e04c38bd976667ab36223f00732a3af01eda255d6ebbeb575b273c554d6aa375d096be755a42813d388f18823e9080bd678e91eda4ce3f855410830feb1643f64dfb968eb16e68100000000000000000000000000000ffffd93ddd094e1f97d330e64458b15b8c89b9469309fa3392389ea7914d6ece7ed2db3a1244c020f544e18a00f66f636272bdde847f91fde214131460d26a3080c859c2dedb23d4182f99c20179db996f952e018739bebbdd0a643257835125cc49094f417dd56e67c65549556a37ce09bb8a91791f7ad1e407d7a1380c88c284eca4d8cab974890400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b671e3853e648bd893b1afd21519616360237691001e6a0a8a0f3dcebed9a4d91265517bd9aea459ebea468eb0c7734b3e3d470530afe8232cb86ba16eaa78cbb6092250af0424b76a3abaddd1ecd2df020000000000000000000000000000ffff0567885a4e2414f2a5efab36b3b205fa374f96692964cb114df9bb45dec65232ea352ad61158f56d09ad508ecca27e88d75553a00ec3f3fa8bab192527e21a828c3aa96c794e58ae38b601180116ccff406f895d7fb7778f6fd5fb6fa1f778ea6a97cd97aade5cda01000000000116ea8db3515ed3755b5898f787663248dcdb675ecf9001495042480c00000000012a1c8466f4a44a7fe0010bfb10b3a51d256f75e3b662b933e2bb6e0200000000012b059731c6a9aa722ccb52107e0e214d971bc2fceb7c804475e7680700000000013c4db348e4a2c2f37905d5b1cbfd2a3a52aa7ffec5c879614fa8e00000000000013c8a5f6c0101cbb0fe3e5e9af871f3c90a1aaa68b1d0834b045f8a0400000000013de093a8efd7aebcfcb14bff15cfdfa072dd7e1807e869d3e9a5a80d0000000001430f69b3bf088319092353104d9de5582f61c98a2f15e45083d0500b00000000014a15b59cdf800839d48851dd8f442b9660d458a6dcddd7d1837220040000000001507b200984632d4aa88ebcb687de36e866ae00a2390b3e71c7bef30100000000015649ef2aedcce8109a2413d228c9ef87954c68f1f7d48cf304838408000000000179cdc7dc9771dbc90f278c749349655505d73e1ae395769a031b1d09000000000185ea712a3657301a5f976444688b1dea80ae5420e84e1695e100860100000000018765775b9426ea4f3a8533a3491453a0160fff197e5cdad6379e2c0b000000000187eada4bbb85a686af020a5f3f6f4acf6e9ebda200012d0d32975f0800000000018d6987ca3e9018be6031ca8c472955a0c7e7b5e37c16917d6112e9130000000001901448fb51c7a5ac592164e8be3b54cc0fe1073d4fcd3a2de7f239030000000001985f6d323753ea0e8d291c357800555cc6e8dbc021aa33125dd8b00a0000000001a47146c50428d4426f21b4bfb54baa3c13ec123b4de866d5eee39c150000000001b7e26ad935bf43904c2b03bfd833be0a8c730d44db110f4c3183af000000000001c354cafa4b8dfe2ebd3214ce04fb80fc958e960dcd295b62998f39000000000001d65f489e33abc9e2e4ddf0d8e510a266580ca24106b4a909de3de40c0000000001f3af3a7af64f5f49eec628869fb379db1e4a024d0d7a3d371fff17080000000001ffc04084719fe130783a58d6f2676d3dab451af07ac03f4ad86faa080000000018010001229c4bf2f4d69677f115f62c78282073d3dea5a84c211c4e2a517d060000000032ffffffffffff0332ffffffffffff030a5e0fe8af096098c11ff30cf4df8004d0bd9ba264f16298e5229936cb1a2c73ac96c48950ec97bab52f204ccc07f571d6c596cda9296c7cb8a8d9f16a21d5e459feb575470296deee1a2f9aba30727097540424624e7675756ee353fb214bd16d4bf437541a99b2f7d4a1b721134678e86edcde6dcf23514f8debdc5eee3a4c03a6744be8f0468f8edda1cdf056da9bda5dfc6ae88641c8da3738b2e7ed64a641ca2199c8ccd27642af627afeb0e3f191f03a27623a74c87a7c5142c31a0c873d1da6cfc8056279998dca4754f89ced6e4638aa4740ff31aacaa7b2b1bc1b8b0f6c1b4ab40e7279881e13f65e9c04a049f95f9e1610b93254e85970750d3f64388c193538c71e512af44957eedaf38e01000122a7a3d6301dc849ae8c0ec56bf1100780a3e511d12101c6145f50080000000032ffffffffffff0332ffffffffffff0304aa1479cc8acc54940cd5d58e56ef21c6458ed0feccbd6f45af2e04766e618274078685028f63e166bc0232ab704950364da4ff9cf9b5a2bc5641993726b6e7d1bd344965259c193b77f3c3e84ca82818f1261a05994084e0a65e32e765e08a80953728dccbb619e6880f471bc4cfcd70b7e9741606df6e1554be12dae0d0a719f7d97cce84179beecb1757619a52d08fcc8def9f07a887b48c5b6f7b45ffc4b2f70449cbd1ff0993f067590299ea4b04db6dc6158cafcd4ff4c2b29000a8b440fd8cfffcf6c0402d387e56e4e2b93818713f96e46ee57b99c6422a0a86abe3133d09922ce7589f11b1224d955addd7295708bab056202fa443b68e72676fddb073490abd102057401a482d7d09290d0100012d86cf8a2fd4d2de16e4b3ecd4bba1c33a32367805ab4fc1c247be0d0000000032ffffffffffff0332ffffffffffff0309b3135b269a5fdefe98fbd888734d4528dd87d57c9025d0d3d851ca90df69702b544c051097fcf5013e2591a96197bbbb474c097a53b63fe10f337a17f0a9b0e0f5d29bc7a3659daffcf3b4290fca9489d24758a98bf3343a0b78af57efaa7758cf5e758d7de480e49fa55ba5ce01e33513e4346728ba1a8b642ca51d2b5e39004b617649838eaffb637cd5da146b30987dddf048e1364d787ed7113df94e1ca68c8f3f95ef63c9840efd88ac31d0cf8ba4155ea1fbbf281e48f449b92d76385f0d5ab151c4748fae6f11852a56b98b1fbcb3fcd89f0927be9471cb7f95720d14822e691bf43cbb97861ee1bf81ac3b1f9956484752078ef042cd4cd36f6108dc52ad4e9696acd835e2888bf637c05201000138169f721527a5a655146a7680d0d20e6ac7be0818fd8ab66a24bc080000000032ffffffffffff0332ffffffffffff0302a916f5736dbbc6c4ff75ec784ebcacdd2a47cc7f78a67bc8b4f172ac6f7510d6f5320887e7bfe553ba38be8a4b42ff0d7f3bda2ac045c160f9107dcd40d4bcb3e003f028d066df1c0ad19554b170de97e9a8c1bf68ab419dcb88d93a7d35426090f7ea6551d3225542b9dcfa97220cbbf6003befebd5146833e4b77a0a0a670ea6156405c04e0341700a3ee70501d01322dd4e305cf52d509c488059bc52d278de341a3378a17b05c7dcf13388bae38d095fa548c755502929afaf1beada57dc8a7da2d51cf97cf9c12911c2ab49051e39d032111ee28455f2071caf10b2ad02c6690898703a0ecac855a18ee7d02bdac7524fea00eead690920a04f967d3eeea86acfc8d6a9c694a9550cb961e8130100013c9a9a918b5d9048208ac3c0aca342dea16af6f647acaca7e8d714000000000032ffffffffffff0332ffffffffffff0388f0db6b93926394e2403df8c0f8578e7a79ff4840726e0ab8a488e0ea29303ce88436e1d9c95985ad634d2a94857f16991b69999679254d67f1b322b879e8ca5579d086d0fc3a207a0d3866481e79ba8f3c934c71ebeee1e5acc136b9639fb6036a2669d3b57ae8fa1ddcac26356ca511998312099fb684a8b939b81fea6bab013b396ae626360c2f678f6cd81a071875c5833dea3cb9152a3dca223ff6d583174325438dc146e8e47c196e243467528e622c90ef10ae687e4be58d13a4552586b8c3c991c148e886f02bae3dd21af85481f1d073ee93fa4d10f76398561c2a12bb503878aada47b9bf56ad06fd632d69c5df05976169f94e08b12cd4eb3fa06ad945f7530496f009dab8883fadff790100013caabc62a671fa3c006491677042282742c32081131aab5b1c55f2040000000032ffffffffffff0332ffffffffffff038337d1cbe6e04fd53cd188a2d9d3fdf512429c2c8d94160dfb422a0955895e5512f3c168d0e8c29c13e29f791a5bc380b7116ee82c478a695d04c0f0d70650eda502dcb7e4d15ccd5109b7f2e269f57c159fd12fecfd806223105e2aeb6184fc89864f09952fb6abb67a9230be74a4bb4ac9763701a5ac7a90ceba8ed4d8761103da982770a8a4ebc7d1e9ffc9790b387dd9d1e4e7970e1defb3cd430e2a9583416c8d894e0978bbd7a84f106a01416999abaab5382f9e71b383c9988c4e7ad536d2a95426cc4b764b3e495dd5b45ff0e2a6f5bad631039f9aedc5b09a01683311f453ee70678e5458f2187f74563ec3a1e2308c04deff959c7de8a9bfbd1d836d70d80f9969b94d51b2a7787cd46e3a0100015d117681bf78ae1fed26bc063fbf06008d0fd56351f769a6b6263b040000000032fffffffffffb0332fffffffffffb0305cc4ab8742fd3d129c86e8be070599f43dbe66b0fd17afe93f29cf4884b9019b3648cf69af90a9ae7aa1e56144ac0f28f14e573cdec4fb46e66aa4cfb3e2b5eb7ba3f634ad3ff6205a84152486b25430c6d4d330de573b9cb74d25c58c11f014a3ccd70671427276fb80287e3ef695823b0856065d144803d2013e0b4e35f8f17948cc4e64db5b5ef37a188c7e9049dc9d9db910613e3c3862522ac758b849955e54e9f56ef8ed12f32b2764383b2dd983093eb0996cbc3ea9a526a9643159b235904c99c7e234c5f100c745245e7fae61a05538230ead59c084ce67776887b0b3f6df1ebd940aaaadbad17ef09dda30512c962efad058ab7eb98cb31dff9efb6e295755220dbfd353abf2c0354270901000170d9854c7682f09f289bcd75cf6505f4b5bad12370d475032d4800000000000032fffffffffffe0332fffffffffffe0392d8fcc39b65236efd68051128d99b6098ec90d25bea351ba68202e236f1bf0aa73d1ab5d738763b154ef0c277421fc211b6dc4d5cdce38c738272d482df5eae8473dd65a822e85546214025ab0985ac93f4baf9a24db3d415a661d82c6c078efd43cc3498b51f2a7f3be0a618c24b42c17f28ae0b6b5ba3310ec51f475583bc020494b12a79af0586d9619001de1c94c1e813e26496b8a8a1e5cee553e5422c6e3099567fd0627ec21ca9bf5f9d12910a004d74df8ba679c575236e7d371b32da1fff031076d395ad4033e6f950d640e3eb6f2e53298f3ed7567f0c8342d6bd0f6390ed7da0668875b6bdef0006452133cdd68f4518b5f2913c856b7d7ed2142300820d5f226fbaa8813260ccae3ce00100017311d9d049c1f7bd326d4d0d45f342340f7840db77208adac758c5090000000032fffffbffffed0332ffffffffffff0302f8d5127f469400542a9d52d16ad9b90d1596f822b1aefd9056d48155ebb993416b6ad00c9c13d1269d7b3e20a5c520d8393c68ab84ac31db33b611a891ea2f5837b8d3f592db5a27a4c1c5399bbef181a5b9a8c83fca1b6d4e6a996f532f2785aa7d9a7b75162c1fa31bf3414a1e93ca0718f18a7e813f4cf93169d7a3c123104e247ac21226893e488428a228747cdf7751566ab05118d9da1c90f26855dd500551afa8c2cb0f2e9cb0c41e99d9d49805fd4610a744d0c8db3d9cedfecf5b98122a8150a64b68c37e0ceda1e5aaaccf44f998e81f4f506126d69c279410fa14dab67581036a3acbd95ca20231f712cfe1f48b4d93576e4417e5cdc2f494f8bd69a868394da16722f347a033094acb010001747c9b153f8aacbd233347d79371cad75525702bb84e4c26bbcc01060000000032ffffffffffff0332ffffffffffff03854d1c416322f76a2b2929e785d3f4507b49c4f43e563a8edf36861fd7c8b863481a7d168424a809331c0da7ff1d983d0e6d82bc2f3d83282b8f1b5328aa06643f9adcff7fb8fef87c8ac2245a912e771441bad128c06149cc7b400d9978cf1465092650e97c288b6de1556c08c410645d885d19558c99a5b0d2ae760cbb789a0167099befc47a825f3c795b7e6feb377ac3241251d08659c96fc1d46fba34999bfeb351ca6099cee491fd3df16dadc01065dd7877dac34930c071de1994fed7d6612ef6a72886767e420e35ba8079dfb411ccbc1bf22d2ae06e5ef33b01082d19e553b5c5c6d6b02cbea959b6b644863adf1ae98157e2a96a7a1f6a9637d158b7234f359a692625b69904f8087863330100017588e7f26743910c556ebf1a565e8335229619245951154b4e20730d0000000032fffdffffffff0332fffdffffffff03960948ddcdadf3e503f9774928333bcb598738139659565d6ba0e0bb293c0087a7a839760c0f8614b78084785ad7adce1b7ae50bb0022441020619de1aaf932405293ec6e8062d50500cce13c7799353044e571f077a5eda2a91d3a41c61f5457d80bfa6a9b43aed520c6aefa4ff908cd59fa7e9bb687584a114f5bbff14d58716ff128d817fcf0bcc7c50474ac7c6b3661e8b502d1f1c366451ad3ea9fddcebf69923a41eaf9ab7435ff839bca8bf2d83fc2c482d5f50636ca951121b2f83aa6c77575b9a8c29464d6c74c46e8435e754061226f4b47c1529a77bae668e06b3109446c9fb5308ed4b2ea93c676109c04ef6be8610e7b00483de2481828414ad15ea9c5cf47fd6e9c45ab77cc3d47b8c010001a855a2b709b073515165f8471f4c3a9264abe8be59764b80538fdf2b0000000032ffffffffffbf0332ffffffffffbf030807fe8cc708ab80facab488b6cc4d1b90e501bfc8b942105f15f74baebcd4589724f89fb0f115815154c0687253b09a99f466e4108a55f8d473435324b124280cc0b920c835959510d0e938d714cbe28126e78d2afd6ef1c9c71e4632d258b2b84b25dc08faeb9674300564bec6e371f075ba348cf2a92793572912d5784a670ff759ee04eeaa74506195b3b69dc059c74328388582cc9945b4646c672b9c6b83b286b3f27a1ed18fc999dabbc8c29981e686edfd62406d1b516437eb28f609bc2a56d5374eeb0dfca61ac33b14f7d99623aee180fb728d7733385e56044cb314a6f63e5b7cff659b9b14d893b28b82a6fef6796c765779d5a2ab0ab345df9aaf2b507764cf9d1415d6f47239517c9b010001abb44f0bcd7959f00e8766b1a689a832259c0bf87f3e42411f1150010000000032ffffffffffff0332ffffffffffff03995fbb3d11d32b2100e7811d8fb0bd262e8edf4d25691fd6c08760845910e459661b3760eb0e3fdb5d413eb4400e8f53cd81735018fa176a29fa073f671e8f0f4651b938e599ae5ad5d954eaea5c166891b131fdb47df2fd282e6bce006c39494af36031510b0194cc9927ef2734ba1d214b3184513c580cf0ab39dbf2211d3b0ee474494123d34e64ab3c21618e864dadb078f030371b94ea35a1c46fca60c738a778acbe633704221e518a8879ef95885107e15f2a385b829712f0334fce99cbdd978349e80d06075aa2036fcacc17661a0b53b70cb6daa5ca3e1410543ef00ade2619124c77e2abdccbe4c817b4f0aad3800c80879c3e0d2c60e81afa7e02d98f3cb946e2fc6f9bc5d5adf3a74096010001b0bc05871d2f1578b91f2c78cdce9e92a5fb6718585187977ae1ba0a0000000032ffffffffffff0332ffffffffffff030bf782d4ea0c1b7995f722f1de4cf655202dcddeedc2f0913c57f29aa6f72806ebddc15f93eef92c2990af727eee415dc42416236dd0c03f82201527f17acb00d60885e43fa920aa43491c6c465e75c300cf0e98f6c7083397f1ae078d5558c792bca461f0bf17dbe5386b5ef5cfbf363bb27c7289909614dcbcf09883672a5007e652b1a690d0a819e60e5acbb8e68feecb2d1569376a492510f778d71262b212997d1045f86f54c3927fe5626ba9ec934abca5e0ae0f99c3eb40ba313318d57dfae76f343ab7c3b6e208415d0fb29a66d99b5831043a45c8043deb3ff2038a06050aab6e4cde88674346d0ae1463bb2314932569b27ce5143051d568cb230602f595cc57fb63468c8430b5ecbe1085010001b79421d25a6e9ed1f9d1c9477b19b4ad31710d7918364d7a7bc922020000000032ffffffffffff0332ffffffffffff0318f1d65cb7b75468a508ee009b086bdf236339f5d6750796dcc740a734f86d406a6b6bb732337122c73103c469542c733463748462774bea3d1f570bdfd001fcac4ee2f42279f2fbd66adedb213b44b60d203236832d3b4983a26cfdb0e80fe74071120778593e9549d49ae9167324c67338fb13ceac7461733424c77eb3e0b90741c84f199914ec2db36f4d8d7afdabe29b58f2c90d180433c9643f2d5c7aba9c6eb05b3d491957b073af7cb9e338c50d228f21887b62b240a537181ff69eddbb6a83f09f45db44c2a965e7e4f75e0e0adca63257db97aadaa3c205659e73270879d781c9aef9940638f4e8aa766615099b7294ee198abedc2614f10cccea8d2b825234c04c8c9e6577ec7ee0637b45010001c51f7a354b8de02094de692028d471a1fc196c0b2b7866fa4c7283080000000032ffffffffffff0332ffffffffffff030dd22046d88a41449a47f443989d4b47e9e40f2b2f397ba4437249c571b3b822cc298fb9edb51dbe1cc7ef3eeb7a19b8e0fbab04bcef15ebfb50b3f185eec18e8352e64682b601e146aee976fce40bff029129774b1dd31e20fd9b2684c1ec24d4cc4d40acfb4065342b5e59a3eb9b85f754dff6d10aad76bfaead32d99d37f514cd613d21606130e21187318502f69fd845f8cb9c20f3afd7442c9715257e368bd7b7f71bb7fb9f22ec6513842e4b0906d9fa3255563d2adf80bf63c97cd3b62ad1e74b7ecca7aed3c25b7fcd5ebabdd128a8f6749e5ad5af801725de10650b10b2dde6ae8f439927d6369a90abc735af6fb86161f44155a95c269a00f51b129f6cbce9b16e099a62fd4b6a5507e1d6010001d2e2629601f122f2f32d9d0753132bb46329ce0f3a080b2b780aac010000000032ffffffffffff0332ffffffffffff0388f2e1a245b157aa9a55f0bb9336d25e99a11c159b881bca36f7946dab1d8be0ea0676f1e55b656aa5622286aaca38e3fa329b291ae781591d5524c8e0b93b2ae83d7cc36ae20d091ae69731cb60e77e0a0424c568f731be57c9b93dd943cd9354530926de7716a8a76039191dde3441c94db26c1837525f84cdb47ae23550c70ee7c1181465db23c1920922b8773e67f771d40fa46831479dfb8306ccb9997c2a303e16fe573ce2a432a6da5fd7dcd70ca9b9333da637a3bcf1af25c5bd868826c6bf2b6da2f3dcd6a9fc7d0281717576228ba955930163937fafec64f3364c032f188166f2a8759268d548fa6822417eac89107396ec1bdc33c74ff3a8e1b491dcb5d74be16f20cdf09d4bae492dc7010001e08bc7fd0ecc0d32351c73289381470c7ca6d8cdfdfd3bd405b247060000000032ffffffffffff0332ffffffffffff0305eaf5657157f44d6b428e1e2a0955bdeec1766edadb7c02002d0b231a2488dc7db1fe1c5164bedcec9574813e1ecec616339956d977239d755c1a31545e01212ae916114c0200f336f112e0c044f25b0d253bfdf2a20009766a9d7a8814343fd18c06214916b68db5d1f275c1be91f87b5a41c2edc25992da7a635637decb0505e0c9f42fc4df27c217a30360abae8f313a2229868f1f0cddb0d197a44c0eaeec70cb1b31a5c2a66052ff37278aa14d95622dd27a00f3c6bae42749761e5fc24d77f3e89bce3588140cda45f9c5d9b07bedfff259f98fe3afdd3854b4d83558152ebdd2ea382578225efc94917520ff3cdbf5c3f4c77a12980a29e44f1ab36c33650f1115cf0857d5cd4182c5dcc0b8010001e93a1a5cc2d3ad72234b21751b534e243653854c4b0cdd15208d01120000000032ffffffffffff0332ffffffffffff0317516b8119d82badfafcb6f3a6714830a1df0d13dd81b3aa24912f43cce663abb3a6c7dbb7636cad0b85a2d3849e629518575db75eedf681b1545a9c70f2f4d46727a169d4b4ed59032a055ac51fc68c97649d0f968172c4ba8a96633efb98ece36326559fc7a7e390e25e88e0c0dd5f37076e0e03168a1de6bd4c7d6f613e6a0f09eb1a69935f7328b19fc6427b73ba34be843ca56d2efe7d0f069e43c18791bf5713b527e20c20e87f2f7ffb01450e8d3b3b3d551612f9f3e04c3fce2dcb802798dbea27db382dadbaba49466fe8fdc72ea620e8ff9aa29143855b6ebd095b1356c3d9b3461bd23bf4d4f1eebfca7bd487e776ef8909be0bbbe6a9db0cd40eec5cb97379c7889ec7c92723b7ffa408010001ec4d8ea1cb816fa14988e6bc1f8938ac90b9ef214d3a537261b81d060000000032ffffffffffff0332ffffffffffff038ff86c7cfe9d23836921884d37a4af5cb1d88c5a930ca1f00529a4445343b9b71be75d7c06a5731037c100fe066ef9a4c55119ce464e6546ba2d656d55ea9a8c15b0f4939ce1b9325e4cabdce34529ad00cf4007211573579dfbfed3e93cd4172d8a5daf54368a1e31889b81d269ca6ee374a560358a9b4a633d64ffa1214f5002e2482b3ee4eed2a92e42d4b901874578f5570506acc302d0e255cb4008bb34847ac4907ca2dd01e56318c12a7b4e0e84607a353985d420c585c1d76aac1ac97f705295b9c5b1637a8611aff3c6305a6522b93c874b3ded3acb050e1482ae24199136df912fc27cd32e4cfb0dfd38d5ce9700c7fd1c0707bf6aa3556d121e652e62adb99c0ab19e493b41b36d1cee96010001ed41a2bae64f1e7179da19e4eb675e2e295b1ca7d30f1c88746a9e0e0000000032ffffffffffff0332ffffffffffff03988bd458f03de72343f737f6feca7062a9c2226c947bc117cd45df9e824880c17d885c9c045d16e1a341afb557851ab23b104969bdd7c6f3ab175258670f9121b70bdf495546b4570de97ba90f7cb42781c78f5c4f958f84bddd69178de0aee0fdfa5c61918c183792bbdfeffbf00c3df06f81167cba06b60e9654ca63f4959e0efccbd371801f8ff0d60db308d8c2abf09909bb49403c9f46de07d19834b1ad3dca5047d4094a566294826c780c35a38e17d593c286ebcee95a60e0bd9e290e646f909cbf1903989ca965756dd9ad13fa225dd1bce3e3dc8746e1a115d21cbe0631c580458496eba0cf25ff55d1e5adae93b9168fb73aa3b0462625e5f8ef07b836279da7a4ccad764e329c9548a7f6010001efa17ae3097aa9c88554ac58ec07b09529d2f464f38b420e62d0d90a0000000032ffffffffffff0332ffffffffffff030b42c50982b77e7e95fbb35216e285999ee986f889ff3d5d989397de11f1f0964e2a7fb61b4a48426f55524c6f59b39147cd4212f001fc31aeb1456843aac9d85bd95fcaf751410e02fd27e86bfbb6b98164d055bad5677965d3d83b93cb8f52010b466a6e02a4b3d8cc6f2403780764d9dca5201f8f7e0c2d508896607848391417e71da7263108993fc049c8dad1fd579d83809acfae8548db95cb3dcb128986ea5c996aaafce2764486c21a7d82dd81234f156a14a79d1cb344bb28cc0ecef519a0c87a7ab5c187a54e4964e63cc94e3fdbd9216ed04ded0aea4a6abee3e9104a2bcdad64507e1043f45671f46a7f146faa79877e4c6bf789e28c5759227df7d4718aee7762e298ba2fd429e54e40010001efc1fa2d7ff2f31687c5d07e48df90bf408a74c2f78d08c7edd4a20a0000000032ffffffffffff0332ffffffffffff030ba2d75b60a28889aac86be1f3916525df469c87f4919b0c396f378f139ea7fbedf327b6e950e6b805aaa84e34c4d3cd0df8094c77bb90379e3c3726797fac83b6e8e67f0a7c3ab76150bb5cbcf3c14e99eee4dc821cd19d752803b0514238911de6fe3ddd9427dab5761470bad0c6dba4b237c8baf6989fba3c52e346f687160d9f1a78ba68470fb59d892b6b79059ced52d090f8e7f5e7b574cfb727e8a44c8d03354cee8ab9b9b1b3d6e8458a98968cfbb720e12c430a3a50661eb9a02770b20dedab3b1328039434641325580d22acd1163f5a377109fdcfd04f2fea442e055d28d24e5d17e650d1bfb188cbaa36b07656fdd22167c4158590538be597330560c2c559bd995fb95472b2cc933da8010001f96c82076e81fd4c978242541bece467baed54c93168782eaa1ada080000000032ffffffffffff0332ffffffffffff0316332cc4a845cebd690dbe138fb1df1e812979e628f366b634ce4ccbc021c879a39befd0c1ab0ab9a9d23459a49ac67a28282e4974ff2e0b5a43c0868d49f46dc54e2be961d479fa2b4fbbe57c666927930caa4d4eb0c9416d12eb570f8ef6a5a8c4f405acd51227c08d2c11084967b251c5bdf8f54b2e7e4199b23ed12f479705b5eb8be17c3f8bff63fdf014e58846077dcfb483b00d29ceb50b3220db02ebcb66e58a4c0466038671a1dc8109876498c4b44abcf11a76fd7d9cf029e2cd2511ad48f6fde37abd4b9c543f042a1330908c2e6bd678b28f70586c5f83dbba37096d58d49834458c661963492c586e9ed5cb6d23c3e9b86bcf0646fb7911d842b11a6f5a905f1d3b61c0da58d1283728";

        let verify_string_hashes = vec!(
        "23cfd922f1991606944a5e178e75e3cca63c3fb7e94720b4e4649403d9116c0c",
        "93f882118caffcec59e7305e5141ced538880a37dbd68e075102f9dc0f51c22b",
        "1e6a0a8a0f3dcebed9a4d91265517bd9aea459ebea468eb0c7734b3e3d470530",
        "4acaabcb7ee31149510138a0b11b087cc5a2ff0a24225bd52b52e1c2c58a113d",
        "79db996f952e018739bebbdd0a643257835125cc49094f417dd56e67c6554955",
        "4431d5c73ecb095daa699363d0a19e5b6e549414e6608cecd1188dbf551a777f",
        "49e7c0077dc7f325ae9402913e681d3a3433e407e8e3f7670d1e0e1f56b8e37f",
        "732a3af01eda255d6ebbeb575b273c554d6aa375d096be755a42813d388f1882",
        "2354b77c0f261f3d5b8424cbe67c2f27130f01c531732a08b8ae3f28aaa1b1fb",
        );

        let verify_string_smle_hashes = vec!(
        "085eca46adf569129effe75c418993dc1e6afe270d8dd15d3bd86a61f0daf7d6",
        "43471bd379d1cd1933d88ef2842a203de9334b0c02976025256bf1fd7df99415",
        "8cde8673f8228131d9ed4f14e920639a226c51a1884724878b433147dd3c8031",
        "07d5bd2519b091171e0de1c685bcbaf63cf34866c068ed600e9192ad248aa72e",
        "65aac8656da4feccf2ca3dccfbc4354846ed97f51c1278022d539dc0977bb614",
        "ab082616b5da66b5614d1b97c3759276ad216944c8ac519d390679aaa0b2056a",
        "065e829bb4c5e3c3890cc7890f7161e896866f740dd5ab9f7800eb87e7927cb9",
        "1c3580556e5642cc25cc674e8cfae5c2b65848b34cac6809ee683ed247323195",
        "860763c4d65bebc2f6b3da76da0e69c62eae470f2d7ff17be192dd266d90c777",
        );

        perform_mnlist_diff_test_for_message(
            hex_string,
            2,
            verify_string_hashes,
            verify_string_smle_hashes,
            block_height_lookup,
            ChainType::TestNet);
    }


    fn perform_mnlist_diff_test_for_message(
        hex_string: &str,
        should_be_total_transactions: u32,
        verify_string_hashes: Vec<&str>,
        verify_string_smle_hashes: Vec<&str>,
        block_height_lookup: wrapped_types::BlockHeightLookup,
        _chain_type: ChainType) {

        let bytes = Vec::from_hex(&hex_string).unwrap();
        let length = bytes.len();
        let c_array = bytes.as_ptr();
        let base_masternode_list = null_mut();
        let merkle_root = [0u8; 32].as_ptr();

        let offset = &mut 0;
        assert!(length - *offset >= 32);
        let base_block_hash = bytes.read_with::<UInt256>(offset, LE).unwrap();
        assert_ne!(base_block_hash, UInt256::default() /*UINT256_ZERO*/, "Base block hash should NOT be empty here");
        assert!(length - *offset >= 32);
        *offset += 32;
        assert!(length - *offset >= 4);
        let total_transactions = bytes.read_with::<u32>(offset, LE).unwrap();
        assert_eq!(total_transactions, should_be_total_transactions, "Invalid transaction count");
        let use_insight_as_backup = false;
        let result = mndiff_process(
            c_array,
            length,
            base_masternode_list,
            masternode_list_lookup,
            masternode_list_destroy,
            merkle_root,
            use_insight_as_backup,
            use_insight_lookup,
            should_process_quorum_of_type,
            validate_quorum_callback,
            block_height_lookup,
            null_mut()
        );
        println!("result: {:?}", result);

        let result = unsafe { unbox_any(result) };
        let masternode_list = unsafe { (*unbox_any(result.masternode_list)).decode() };
        let masternodes = masternode_list.masternodes.clone();
        let mut pro_tx_hashes: Vec<UInt256> = masternodes.clone().into_keys().collect();
        pro_tx_hashes.sort();
        let mut verify_hashes: Vec<UInt256> = verify_string_hashes
            .into_iter()
            .map(|h|
                Vec::from_hex(h)
                    .unwrap()
                    .read_with::<UInt256>(&mut 0, byte::LE)
                    .unwrap()
                    .reversed()
            )
            .collect();
        verify_hashes.sort();
        assert_eq!(verify_hashes, pro_tx_hashes, "Provider transaction hashes");

        let mut masternode_list_hashes: Vec<UInt256> = pro_tx_hashes
            .clone()
            .iter()
            .map(|hash| masternodes[hash].masternode_entry_hash)
            .collect();
        masternode_list_hashes.sort();

        let mut verify_smle_hashes: Vec<UInt256> = verify_string_smle_hashes
            .into_iter()
            .map(|h|
                Vec::from_hex(h)
                    .unwrap()
                    .read_with::<UInt256>(&mut 0, byte::LE)
                    .unwrap())
            .collect();
        verify_smle_hashes.sort();

        assert_eq!(masternode_list_hashes, verify_smle_hashes, "SMLE transaction hashes");

        assert!(result.has_found_coinbase, "The coinbase was not part of provided hashes");
    }

    #[test]
    fn test_mnl_saving_to_disk() { // testMNLSavingToDisk
        let executable = env::current_exe().unwrap();
        let path = match executable.parent() {
            Some(name) => name,
            _ => panic!()
        };
        let filepath = format!("{}/../../../src/{}", path.display(), "ML_at_122088.dat");
        println!("{:?}", filepath);
        let file = get_file_as_byte_vec(&filepath);
        let bytes = file.as_slice();
        let length = bytes.len();
        let c_array = bytes.as_ptr();
        let base_masternode_list = null_mut();
        let merkle_root = [0u8; 32].as_ptr();
        let use_insight_as_backup= false;
        let result = mndiff_process(
            c_array,
            length,
            base_masternode_list,
            masternode_list_lookup,
            masternode_list_destroy,
            merkle_root,
            use_insight_as_backup,
            use_insight_lookup,
            should_process_quorum_of_type,
            validate_quorum_callback,
            block_height_lookup,
            null_mut()
        );
        println!("{:?}", result);

        let result = unsafe { unbox_any(result) };
        let masternode_list = unsafe { (*unbox_any(result.masternode_list)).decode() };
        let masternode_list_merkle_root = UInt256::from_hex("94d0af97187af3b9311c98b1cf40c9c9849df0af55dc63b097b80d4cf6c816c5").unwrap();
        let obtained_mn_merkle_root = masternode_list.masternode_merkle_root.unwrap();
        let equal = masternode_list_merkle_root == obtained_mn_merkle_root;
        assert!(equal, "MNList merkle root should be valid");
        assert!(result.has_found_coinbase, "Did not find coinbase at height {}", BLOCK_HEIGHT);
        // turned off on purpose as we don't have the coinbase block
        // assert!(result.valid_coinbase, "Coinbase not valid at height {}", BLOCK_HEIGHT);
        assert!(result.has_valid_mn_list_root, "rootMNListValid not valid at height {}", BLOCK_HEIGHT);
        assert!(result.has_valid_quorum_list_root, "rootQuorumListValid not valid at height {}", BLOCK_HEIGHT);
        assert!(result.has_valid_quorums, "validQuorums not valid at height {}", BLOCK_HEIGHT);
    }

    #[test]
    fn test_quorum_rotation() {

    }

    #[test]
    fn test_multiple_merkle_hashes() {
        let merkle_hashes = Vec::from_hex("78175171f830d9ea3e67170dfdec6bd805d31b22b19eaf783355adae06faa3539762500f0eca01a59f0e198522a0752f96be9032803fb21311a992089b9472bd1361a2db43a580e40f81bd5e17eabae8eebb02e9a651ae348d88d51ca824df19").unwrap();
        let merkle_flags = Vec::from_hex("07").unwrap();
        let desired_merkle_root = UInt256::from_hex("bd6a344573ba1d6faf24f021324fa3360562404536246503c4cba372f94bfa4a").unwrap();
        let total_transactions = 4;
        let merkle_tree = MerkleTree {
            tree_element_count: total_transactions,
            hashes: merkle_hashes.as_slice(),
            flags: merkle_flags.as_slice(),
        };

        let has_valid_coinbase = merkle_tree.has_root(desired_merkle_root);
        println!("merkle_tree: {:?} {:?} {}, has_valid_coinbase: {} {:?}", merkle_hashes.to_hex(), merkle_flags.to_hex(), total_transactions, has_valid_coinbase, desired_merkle_root);
        assert!(has_valid_coinbase, "Invalid coinbase here");
    }
}

