use std::cmp::min;
use std::collections::{BTreeMap, HashSet};
use dash_spv_ffi::ffi::boxer::{boxed, boxed_vec};
use dash_spv_ffi::ffi::callbacks::{lookup_block_hash_by_height, lookup_masternode_list, lookup_snapshot};
use dash_spv_ffi::types;
use dash_spv_models::common;
use dash_spv_models::common::{LLMQSnapshotSkipMode, LLMQType};
use dash_spv_models::llmq;
use dash_spv_models::masternode::{LLMQEntry, MasternodeEntry, MasternodeList};
use dash_spv_primitives::consensus::{Encodable, encode};
use dash_spv_primitives::crypto::byte_util::{Reversable, Zeroable};
use dash_spv_primitives::crypto::data_ops::{Data, inplace_intersection};
use dash_spv_primitives::crypto::UInt256;
use dash_spv_primitives::hashes::{Hash, sha256d};

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ConsensusType {
    MN = 0,
    LLMQ = 1,
    LlmqRotation = 2
}

pub struct LLMQValidationParams {
    pub consensus_type: ConsensusType,
}

#[derive(Clone, Copy, Debug)]
pub struct Manager<
    BHH: Fn(UInt256) -> u32 + Copy,
    BHT: Fn(u32) -> *const u8 + Copy,
    SL: Fn(u32) -> *const types::LLMQSnapshot + Copy,
    MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
    MND: Fn(*const types::MasternodeList) + Copy,
    AI: Fn(UInt256) + Copy,
    SPQ: Fn(LLMQType) -> bool + Copy,
    VQ: Fn(types::LLMQValidationData) -> bool + Copy,
> {
    pub get_block_height_by_hash: BHH,
    pub get_block_hash_by_height: BHT,
    pub masternode_list_lookup: MNL,
    pub masternode_list_destroy: MND,
    pub add_insight_lookup: AI,
    pub should_process_llmq_of_type: SPQ,
    pub validate_llmq_callback: VQ,
    pub use_insight_as_backup: bool,
    pub base_masternode_list_hash: Option<UInt256>,
    pub consensus_type: ConsensusType,
    pub get_snapshot_by_block_height: SL,
}


pub fn lookup_masternodes_and_quorums_for<MNL, MND>(
    block_hash: Option<UInt256>,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND,
) -> (BTreeMap<UInt256, MasternodeEntry>, BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>>)
    where
        MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
        MND: Fn(*const types::MasternodeList) + Copy,
{
    if let Some(block_hash) = block_hash {
        if let Some(list) = lookup_masternode_list(block_hash, masternode_list_lookup, masternode_list_destroy) {
            return (list.masternodes, list.quorums);
        }
    }
    (BTreeMap::new(), BTreeMap::new())
}


pub fn classify_masternodes(base_masternodes: BTreeMap<UInt256, MasternodeEntry>,
                        added_or_modified_masternodes: BTreeMap<UInt256, MasternodeEntry>,
                        deleted_masternode_hashes: Vec<UInt256>,
                        block_height: u32,
                        block_hash: UInt256)
                        -> (BTreeMap<UInt256, MasternodeEntry>,
                            BTreeMap<UInt256, MasternodeEntry>,
                            BTreeMap<UInt256, MasternodeEntry>) {
    let added_or_modified_masternodes = added_or_modified_masternodes;
    let deleted_masternode_hashes = deleted_masternode_hashes;
    let mut added_masternodes = added_or_modified_masternodes.clone();
    let mut modified_masternode_keys: HashSet<UInt256> = HashSet::new();
    if base_masternodes.len() > 0 {
        let base_masternodes = base_masternodes.clone();
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
            acc.insert(hash, added_or_modified_masternodes[&hash].clone());
            acc
        });
    let mut masternodes = if base_masternodes.len() > 0 {
        let mut old_mnodes = base_masternodes.clone();
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
            if (*old).update_height < (*modified).update_height {
                if (*modified).provider_registration_transaction_hash == (*old).provider_registration_transaction_hash {
                    (*modified).update_with_previous_entry(old, common::Block { height: block_height, hash: block_hash });
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
    (added_masternodes, modified_masternodes, masternodes)
}

pub fn classify_quorums<
    MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
    MND: Fn(*const types::MasternodeList) + Copy,
    AI: Fn(UInt256) + Copy,
    SPQ: Fn(LLMQType) -> bool + Copy,
    BHH: Fn(UInt256) -> u32 + Copy,
    BHT: Fn(u32) -> *const u8 + Copy,
    SL: Fn(u32) -> *const types::LLMQSnapshot + Copy,
    VQ: Fn(types::LLMQValidationData) -> bool + Copy,
>(
    base_quorums: BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>>,
    added_quorums: BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>>,
    deleted_quorums: BTreeMap<LLMQType, Vec<UInt256>>,
    manager: Manager<BHH, BHT, SL, MNL, MND, AI, SPQ, VQ>,
)
    -> (BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>>,
        BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>>,
        bool,
        Vec<*mut [u8; 32]>
    ) {
    let has_valid_quorums = true;
    let mut needed_masternode_lists: Vec<*mut [u8; 32]> = Vec::new();
    added_quorums
        .iter()
        .for_each(|(&llmq_type, llmqs_of_type)| {
            if (manager.should_process_llmq_of_type)(llmq_type) {
                (*llmqs_of_type).iter().for_each(|(&llmq_block_hash, quorum)| {
                    match lookup_masternode_list(llmq_block_hash, manager.masternode_list_lookup, manager.masternode_list_destroy) {
                        Some(llmq_masternode_list) =>
                            validate_quorum(
                                quorum.clone(),
                                has_valid_quorums,
                                llmq_masternode_list,
                                manager.get_block_height_by_hash,
                                manager.get_block_hash_by_height,
                                manager.get_snapshot_by_block_height,
                                manager.masternode_list_lookup,
                                manager.masternode_list_destroy,
                                manager.validate_llmq_callback,
                                manager.consensus_type),
                        None =>
                            if (manager.get_block_height_by_hash)(llmq_block_hash) != u32::MAX {
                                needed_masternode_lists.push(boxed(llmq_block_hash.0));
                            } else if manager.use_insight_as_backup {
                                (manager.add_insight_lookup)(llmq_block_hash);
                                if (manager.get_block_height_by_hash)(llmq_block_hash) != u32::MAX {
                                    needed_masternode_lists.push(boxed(llmq_block_hash.0));
                                }
                            }
                    }
                });
            }
    });
    let mut quorums = base_quorums.clone();
    quorums.extend(added_quorums
        .clone()
        .into_iter()
        .filter(|(key, _entries)| !quorums.contains_key(key))
        .collect::<BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>>>());
    quorums.iter_mut().for_each(|(llmq_type, llmq_map)| {
        if let Some(keys_to_delete) = deleted_quorums.get(llmq_type) {
            keys_to_delete.into_iter().for_each(|key| {
                (*llmq_map).remove(key);
            });
        }
        if let Some(keys_to_add) = added_quorums.get(llmq_type) {
            keys_to_add.clone().into_iter().for_each(|(key, entry)| {
                (*llmq_map).insert(key, entry);
            });
        }
    });
    (added_quorums, quorums, has_valid_quorums, needed_masternode_lists)
}


#[cfg(test)]
fn log_quorums_map(q: BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>>, id: String) {
    println!("{} hashes: [", id);
    let mut bmap: BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>> = BTreeMap::new();
    for (qtype, map) in q.clone() {
        let qqtype: u8 = qtype.into();
        let mut bhmap: BTreeMap<UInt256, LLMQEntry> = BTreeMap::new();
        for (hash, entry) in map {
            bhmap.insert(hash, entry);
        }
        bmap.insert(qtype, bhmap);
    }
    for (qtype, map) in bmap {
        let qqtype: u8 = qtype.into();
        for (hash, entry) in map {
            println!("{}:{}", qqtype, hash);
        }
    }
    println!("]");
}

pub fn validate_quorum<
    BHH: Fn(UInt256) -> u32,
    BHT: Fn(u32) -> *const u8 + Copy,
    SL: Fn(u32) -> *const types::LLMQSnapshot + Copy,
    MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
    MND: Fn(*const types::MasternodeList) + Copy,
    VQ: Fn(types::LLMQValidationData) -> bool + Copy,
>(
    mut quorum: LLMQEntry,
    mut has_valid_quorums: bool,
    llmq_masternode_list: MasternodeList,
    get_block_height_by_hash: BHH,
    get_block_hash_by_height: BHT,
    get_snapshot_by_block_height: SL,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND,
    validate_llmq_callback: VQ,
    consensus_type: ConsensusType,
) {
    let block_height: u32 = get_block_height_by_hash(llmq_masternode_list.block_hash);
    let quorum_modifier = quorum.llmq_quorum_hash();
    let quorum_count = quorum.llmq_type.size();
    let valid_masternodes = if consensus_type == ConsensusType::LlmqRotation {
        //valid_masternodes_for_rotated_llmq(llmq_masternode_list.masternodes, block_height)
        get_rotated_masternodes_for_quorum(quorum.llmq_type, llmq_masternode_list.block_hash, get_block_height_by_hash, get_block_hash_by_height, get_snapshot_by_block_height, masternode_list_lookup, masternode_list_destroy)
    } else {
        valid_masternodes_for(llmq_masternode_list.masternodes, quorum_modifier, quorum_count, block_height)
    };
    let operator_pks: Vec<*mut [u8; 48]> = (0..valid_masternodes.len())
        .into_iter()
        .filter_map(|i| match quorum.signers_bitset.bit_is_true_at_le_index(i as u32) {
            true => Some(boxed(valid_masternodes[i].operator_public_key_at(block_height).0)),
            false => None
        })
        .collect();
    let operator_public_keys_count = operator_pks.len();
    let is_valid_signature = validate_llmq_callback(types::LLMQValidationData {
        items: boxed_vec(operator_pks),
        count: operator_public_keys_count,
        commitment_hash: boxed(quorum.generate_commitment_hash().0),
        all_commitment_aggregated_signature: boxed(quorum.all_commitment_aggregated_signature.0),
        threshold_signature: boxed(quorum.threshold_signature.0),
        public_key: boxed(quorum.public_key.0)
    });
    has_valid_quorums &= quorum.validate_payload() && is_valid_signature;
    if has_valid_quorums {
        quorum.verified = true;
    }
}

pub fn score_masternodes(masternodes: Vec<MasternodeEntry>, quorum_modifier: UInt256, block_height: u32) -> BTreeMap<UInt256, MasternodeEntry> {
    masternodes
        .into_iter()
        .fold(BTreeMap::new(),|mut map, entry| {
            match MasternodeList::masternode_score(entry.clone(), quorum_modifier, block_height) {
                Some(score) => {
                    if !score.0.is_empty() {
                        map.insert(score, entry);
                    }
                },
                None => {}
            };
            map
        })
}
pub fn score_masternodes_map(masternodes: BTreeMap<UInt256, MasternodeEntry>, quorum_modifier: UInt256, block_height: u32) -> BTreeMap<UInt256, MasternodeEntry> {
    masternodes.clone().into_iter().filter_map(|(_, entry)| {
        let score = MasternodeList::masternode_score(entry.clone(), quorum_modifier, block_height);
        if score.is_some() && !score.unwrap().0.is_empty() {
            Some((score.unwrap(), entry))
        } else {
            None
        }
    }).collect()
}


pub fn get_valid_masternodes(mut scored_masternodes: BTreeMap<UInt256, MasternodeEntry>, quorum_count: u32, masternodes_in_list_count: usize, block_height: u32) -> Vec<MasternodeEntry> {
    let mut scores: Vec<UInt256> = scored_masternodes.clone().into_keys().collect();
    scores.sort_by(|&s1, &s2| s2.clone().reversed().cmp(&s1.clone().reversed()));
    let mut valid_masternodes: Vec<MasternodeEntry> = Vec::new();
    let count = min(masternodes_in_list_count, scores.len());
    for i in 0..count {
        if let Some(masternode) = scored_masternodes.get_mut(&scores[i]) {
            if (*masternode).is_valid_at(block_height) {
                valid_masternodes.push((*masternode).clone());
            }
        }
        if valid_masternodes.len() == quorum_count as usize {
            break;
        }
    }
    valid_masternodes
}

pub fn valid_masternodes_for_quorum(masternodes: Vec<MasternodeEntry>, quorum_modifier: UInt256, quorum_count: u32, block_height: u32) -> Vec<MasternodeEntry> {
    let masternodes_in_list_count = masternodes.len();
    let score_dictionary = score_masternodes(masternodes, quorum_modifier, block_height);
    get_valid_masternodes(score_dictionary, quorum_count, masternodes_in_list_count, block_height)
}

pub fn valid_masternodes_for(masternodes: BTreeMap<UInt256, MasternodeEntry>, quorum_modifier: UInt256, quorum_count: u32, block_height: u32) -> Vec<MasternodeEntry> {
    let masternodes_in_list_count = masternodes.len();
    let score_dictionary = score_masternodes_map(masternodes, quorum_modifier, block_height);
    get_valid_masternodes(score_dictionary, quorum_count, masternodes_in_list_count, block_height)
}

// Same as in LLMQEntry
// TODO: migrate to LLMQEntry
fn build_llmq_modifier(llmq_type: LLMQType, block_hash: UInt256) -> UInt256 {
    let mut buffer: Vec<u8> = Vec::with_capacity(33);
    let offset: &mut usize = &mut 0;
    *offset += encode::VarInt(llmq_type as u64).consensus_encode(&mut buffer).unwrap();
    *offset += block_hash.consensus_encode(&mut buffer).unwrap();
    UInt256(sha256d::Hash::hash(&buffer).into_inner())
}

// Quorum members dichotomy in snapshot
fn masternode_usage_by_snapshot<BHT, MNL, MND>(
    llmq_type: LLMQType,
    quorum_base_block: common::Block,
    snapshot: llmq::LLMQSnapshot,
    block_hash_lookup: BHT,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND) -> (Vec<MasternodeEntry>, Vec<MasternodeEntry>) // (used , unused)
    where
        BHT: Fn(u32) -> *const u8 + Copy,
        MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
        MND: Fn(*const types::MasternodeList) + Copy {
    let block_height = quorum_base_block.height - 8;
    match lookup_block_hash_by_height(block_height, block_hash_lookup) {
        None => panic!("missing hash for block at height: {}", block_height),
        Some(block_hash) =>
            match lookup_masternode_list(block_hash, masternode_list_lookup, masternode_list_destroy) {
                None => panic!("missing masternode list for block at height: {} with hash: {}", block_height, block_hash),
                Some(masternode_list) => {
                    let nodes = valid_masternodes_for(masternode_list.masternodes, build_llmq_modifier(llmq_type, block_hash), llmq_type.active_quorum_count(), block_height);
                    let mut i: u32 = 0;
                    nodes
                        .into_iter()
                        .partition(|_| {
                            let is_true = snapshot.member_list.bit_is_true_at_le_index(i);
                            i += 1;
                            is_true
                        })
            }
        }
    }
}

// Reconstruct quorum members at index from snapshot
pub fn quorum_quarter_members_by_snapshot<BHT, MNL, MND>(
    llmq_params: common::LLMQParams,
    quorum_base_block: common::Block,
    snapshot: llmq::LLMQSnapshot,
    get_block_hash_by_height: BHT,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND) -> Vec<Vec<MasternodeEntry>>
    where
        BHT: Fn(u32) -> *const u8 + Copy,
        MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
        MND: Fn(*const types::MasternodeList) + Copy, {
    let llmq_type = llmq_params.r#type;
    let quorum_count = llmq_params.signing_active_quorum_count;
    let quorum_size = llmq_params.size;
    let quarter_size = (quorum_size / 4) as usize;
    let work_block_height = quorum_base_block.height - 8;
    let work_block_hash = lookup_block_hash_by_height(work_block_height, get_block_hash_by_height).unwrap();
    let quorum_modifier = build_llmq_modifier(llmq_params.r#type, work_block_hash);
    let (used_at_h, unused_at_h) =
        masternode_usage_by_snapshot(llmq_type, quorum_base_block, snapshot.clone(), get_block_hash_by_height, masternode_list_lookup, masternode_list_destroy);
    let mut sorted_combined_mns_list = valid_masternodes_for_quorum(unused_at_h, quorum_modifier, quorum_count, work_block_height);
    sorted_combined_mns_list.extend(valid_masternodes_for_quorum(used_at_h, quorum_modifier, quorum_count, work_block_height));
    let quorum_num = quorum_count as usize;
    let mut quarter_quorum_members = Vec::<Vec<MasternodeEntry>>::with_capacity(quorum_num);
    let skip_list = snapshot.skip_list;
    match snapshot.skip_list_mode {
        // No skipping. The skip list is empty.
        LLMQSnapshotSkipMode::NoSkipping => {
            let mut iter = sorted_combined_mns_list.iter();
            (0..quorum_num).for_each(|i| {
                let mut quarter = Vec::<MasternodeEntry>::new();
                while quarter.len() < quarter_size {
                    if let Some(node) = iter.next() {
                        quarter.push(node.clone());
                    } else {
                        iter = sorted_combined_mns_list.iter();
                    }
                }
                quarter_quorum_members.push(quarter);
            });
        },
        // Skip the first entry of the list.
        // The following entries contain the relative position of subsequent skips.
        // For example, if during the initialization phase you skip entries x, y and z of the masternode
        // list, the skip list will contain x, y-x and z-y in this mode.
        LLMQSnapshotSkipMode::SkipFirst => {
            let mut first_entry_index = 0;
            let mut processed_skip_list = Vec::<u32>::new();
            skip_list.iter().for_each(|s| {
                let index = first_entry_index + s;
                if first_entry_index == 0 {
                    first_entry_index = *s;
                }
                processed_skip_list.push(index)
            });
            let mut index: usize = 0;
            let mut idxk: usize = 0;
            (0..quorum_num).for_each(|i| {
                let mut quarter = Vec::<MasternodeEntry>::new();
                while quarter.len() < quarter_size {
                    if let Some(skipped) = processed_skip_list.get(idxk) {
                        idxk += 1;
                    } else if let Some(node) = sorted_combined_mns_list.get(index) {
                        quarter.push(node.clone());
                        index += 1;
                        if index == sorted_combined_mns_list.len() {
                            index = 0;
                        }
                    }
                }
                quarter_quorum_members.push(quarter);
            });
        },
        // Contains the entries which were not skipped.
        // This is better when there are many skips.
        // Mode 2 is more efficient and should be used when 3/4*quorumSize ≥ 1/2*masternodeNb or
        // quorumsize ≥ 2/3*masternodeNb
        LLMQSnapshotSkipMode::SkipExcept => {
            (0..quorum_num).for_each(|i| {
                let mut quarter = Vec::<MasternodeEntry>::new();
                skip_list.iter().for_each(|unskipped| {
                    if let Some(node) = sorted_combined_mns_list.get(*unskipped as usize) {
                        if quarter.len() < quarter_size {
                            quarter.push(node.clone());
                        }
                    }
                });
                quarter_quorum_members.push(quarter);
            });
        },
        LLMQSnapshotSkipMode::SkipAll => {},
    }
    quarter_quorum_members.clone()
}

// Determine quorum members at new index
pub fn new_quorum_quarter_members<BHT, MNL, MND>(
    params: common::LLMQParams,
    quorum_base_block: common::Block,
    previous_quarters: [Vec<Vec<MasternodeEntry>>; 3],
    get_block_hash_by_height: BHT,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND) -> Vec<Vec<MasternodeEntry>>
    where
        BHT: Fn(u32) -> *const u8 + Copy,
        MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
        MND: Fn(*const types::MasternodeList) + Copy, {
    let quorum_count = params.signing_active_quorum_count;
    let num_quorums = quorum_count as usize;
    let mut quarter_quorum_members = Vec::<Vec<MasternodeEntry>>::with_capacity(num_quorums);
    let quorum_size = params.size as usize;
    let quarter_size = quorum_size / 4;
    let work_block_height = quorum_base_block.height - 8;
    let work_block_hash = lookup_block_hash_by_height(work_block_height, get_block_hash_by_height).unwrap();
    let modifier = build_llmq_modifier(params.r#type, work_block_hash);
    match lookup_masternode_list(work_block_hash, masternode_list_lookup, masternode_list_destroy) {
        None => panic!("missing masternode list for height: {} / -8:{}", quorum_base_block.height, work_block_height),
        Some(masternode_list) => {
            if masternode_list.masternodes.len() < quarter_size {
                quarter_quorum_members
            } else {
                let mut masternodes_used_at_h = Vec::<MasternodeEntry>::new();
                let mut masternodes_unused_at_h = Vec::<MasternodeEntry>::new();
                let mut masternodes_used_at_h_index = Vec::<Vec<MasternodeEntry>>::with_capacity(num_quorums);
                (0..num_quorums).into_iter().for_each(|i| {
                    // for quarters h - c, h -2c, h -3c
                    previous_quarters.iter().for_each(|q| {
                        q.get(i).unwrap().iter().for_each(|node| {
                            if node.is_valid {
                                masternodes_used_at_h.push(node.clone());
                                masternodes_used_at_h_index[i].push(node.clone());
                            }
                        });
                    });
                });
                masternode_list.masternodes.into_values().for_each(|mn| {
                    if mn.is_valid &&
                        masternodes_unused_at_h
                            .iter()
                            .filter(|node| mn.provider_registration_transaction_hash == node.provider_registration_transaction_hash)
                            .count() == 0 {
                        masternodes_unused_at_h.push(mn);
                    }
                });
                let mut sorted_combined_mns_list = valid_masternodes_for_quorum(masternodes_unused_at_h, modifier, quorum_count, work_block_height);
                sorted_combined_mns_list.extend(valid_masternodes_for_quorum(masternodes_used_at_h, modifier, quorum_count, work_block_height));
                let mut skip_list = Vec::<usize>::new();
                let mut first_skipped_index = 0;
                let mut idx = 0;
                (0..num_quorums).for_each(|i| {
                    while quarter_quorum_members.get(i).unwrap().len() < quarter_size {
                        let mn = sorted_combined_mns_list.get(idx).unwrap();
                        if masternodes_used_at_h_index
                            .get(i)
                            .unwrap()
                            .into_iter()
                            .filter(|&node| mn.provider_registration_transaction_hash == node.provider_registration_transaction_hash)
                            .count() == 0 {
                            quarter_quorum_members.get_mut(i).unwrap().push(mn.clone());
                        } else {
                            let skip_index = idx - first_skipped_index;
                            if first_skipped_index == 0 {
                                first_skipped_index = idx;
                            }
                            skip_list.push(idx);
                        }
                        idx += 1;
                        if idx == sorted_combined_mns_list.len() {
                            idx = 0;
                        }
                    }
                });
                quarter_quorum_members
            }
        }
    }
}

pub fn quorum_members_by_quarter_rotation<BHT, SL, MNL, MND>(
    llmq_type: LLMQType,
    quorum_base_block: common::Block,
    get_block_hash_by_height: BHT,
    snapshot_lookup: SL,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND) -> Vec<Vec<MasternodeEntry>>
    where
        BHT: Fn(u32) -> *const u8 + Copy,
        SL: Fn(u32) -> *const types::LLMQSnapshot + Copy,
        MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
        MND: Fn(*const types::MasternodeList) + Copy {
    let llmq_params = llmq_type.params();
    let num_quorums = llmq_params.signing_active_quorum_count as usize;
    let cycle_length = llmq_params.dkg_params.interval;
    let block_m_c_height = quorum_base_block.height - cycle_length;
    let block_m_2c_height = quorum_base_block.height - 2 * cycle_length;
    let block_m_3c_height = quorum_base_block.height - 3 * cycle_length;
    let block_m_c = common::Block { height: block_m_c_height, hash: lookup_block_hash_by_height(block_m_c_height, get_block_hash_by_height).unwrap() };
    let block_m_2c = common::Block { height: block_m_2c_height, hash: lookup_block_hash_by_height(block_m_2c_height, get_block_hash_by_height).unwrap() };
    let block_m_3c = common::Block { height: block_m_3c_height, hash: lookup_block_hash_by_height(block_m_3c_height, get_block_hash_by_height).unwrap() };
    let q_snapshot_h_m_c = lookup_snapshot(block_m_c.height - 8, snapshot_lookup).unwrap();
    let q_snapshot_h_m_2c = lookup_snapshot(block_m_2c.height - 8, snapshot_lookup).unwrap();
    let q_snapshot_h_m_3c = lookup_snapshot(block_m_3c.height - 8, snapshot_lookup).unwrap();
    let prev_q_h_m_c = quorum_quarter_members_by_snapshot(llmq_params, block_m_c, q_snapshot_h_m_c, get_block_hash_by_height, masternode_list_lookup, masternode_list_destroy);
    let prev_q_h_m_2c = quorum_quarter_members_by_snapshot(llmq_params, block_m_2c, q_snapshot_h_m_2c, get_block_hash_by_height, masternode_list_lookup, masternode_list_destroy);
    let prev_q_h_m_3c = quorum_quarter_members_by_snapshot(llmq_params, block_m_3c, q_snapshot_h_m_3c, get_block_hash_by_height, masternode_list_lookup, masternode_list_destroy);
    let mut quorum_members = Vec::<Vec<MasternodeEntry>>::with_capacity(num_quorums);
    let new_quarter_members = new_quorum_quarter_members(llmq_params, quorum_base_block, [prev_q_h_m_c.clone(), prev_q_h_m_2c.clone(), prev_q_h_m_3c.clone()], get_block_hash_by_height, masternode_list_lookup, masternode_list_destroy);
    (0..num_quorums).for_each(|i| {
        add_quorum_members_from_quarter(&mut quorum_members, &prev_q_h_m_3c, i);
        add_quorum_members_from_quarter(&mut quorum_members, &prev_q_h_m_2c, i);
        add_quorum_members_from_quarter(&mut quorum_members, &prev_q_h_m_c, i);
        add_quorum_members_from_quarter(&mut quorum_members, &new_quarter_members, i);
    });
    quorum_members
}

fn add_quorum_members_from_quarter(quorum_members: &mut Vec<Vec<MasternodeEntry>>, quarter: &Vec<Vec<MasternodeEntry>>, index: usize) {
    if let Some(indexed_quarter) = quarter.get(index) {
        indexed_quarter.iter().for_each(|member| {
            if let Some(quarter_members) = quorum_members.get_mut(index) {
                quarter_members.push(member.clone());
            } else {
                quorum_members.insert(index, vec![member.clone()]);
            }
        })
    }
}

// Determine masternodes which is responsible for signing at this quorum index
pub fn get_rotated_masternodes_for_quorum<BHH, BHT, SL, MNL, MND>(
    llmq_type: LLMQType,
    quorum_base_block_hash: UInt256,
    get_block_height_by_hash: BHH,
    get_block_hash_by_height: BHT,
    snapshot_lookup: SL,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND) -> Vec<MasternodeEntry>
    where
        BHH: Fn(UInt256) -> u32,
        BHT: Fn(u32) -> *const u8 + Copy,
        SL: Fn(u32) -> *const types::LLMQSnapshot + Copy,
        MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
        MND: Fn(*const types::MasternodeList) + Copy {
    // TODO: load members from cache
    let mut map_quorum_members = BTreeMap::<LLMQType, BTreeMap<UInt256, Vec<MasternodeEntry>>>::new();
    let mut map_indexed_quorum_members = BTreeMap::<LLMQType, BTreeMap<llmq::LLMQIndexedHash, Vec<MasternodeEntry>>>::new();

    let map_by_type_opt = map_quorum_members.get_mut(&llmq_type);
    if map_by_type_opt.is_some() {
        if let Some(members) = map_by_type_opt.as_ref().unwrap().get(&quorum_base_block_hash) {
            return members.clone();
        }
    }
    let map_by_type = map_by_type_opt.unwrap();
    let mut quorum_members = Vec::<MasternodeEntry>::new();
    let quorum_base_block_height = get_block_height_by_hash(quorum_base_block_hash);
    let quorum_index = quorum_base_block_height % llmq_type.params().dkg_params.interval;
    let cycle_quorum_base_block_height = quorum_base_block_height - quorum_index;
    let cycle_quorum_base_block_hash = lookup_block_hash_by_height(cycle_quorum_base_block_height, get_block_hash_by_height).unwrap();
    let cycle_quorum_base_block = common::Block { height: cycle_quorum_base_block_height, hash: cycle_quorum_base_block_hash };
    if let Some(map_by_type_indexed) = map_indexed_quorum_members.get(&llmq_type) {
        let cycle_indexed_hash = llmq::LLMQIndexedHash { hash: cycle_quorum_base_block_hash, index: quorum_index };
        if let Some(indexed_members) = map_by_type_indexed.get(&cycle_indexed_hash) {
            quorum_members = indexed_members.clone();
            map_by_type.insert(cycle_quorum_base_block_hash, quorum_members.clone());
            return quorum_members;
        }
    }
    let rotated_members = quorum_members_by_quarter_rotation(llmq_type, cycle_quorum_base_block, get_block_hash_by_height, snapshot_lookup, masternode_list_lookup, masternode_list_destroy);
    let map_indexed_quorum_members_of_type = map_indexed_quorum_members.get_mut(&llmq_type).unwrap();
    rotated_members.iter().enumerate().for_each(|(i, members)| {
        let indexed_hash = llmq::LLMQIndexedHash { hash: cycle_quorum_base_block_hash, index: i as u32 };
        map_indexed_quorum_members_of_type.insert(indexed_hash, members.clone());
    });
    if let Some(members) = rotated_members.get(quorum_index as usize) {
        quorum_members = members.clone();
        map_by_type.insert(quorum_base_block_hash, quorum_members.clone());
    }
    quorum_members
}
