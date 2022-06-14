use std::cmp::min;
use std::collections::{BTreeMap, HashMap, HashSet};
use dash_spv_ffi::ffi::boxer::{boxed, boxed_vec};
use dash_spv_ffi::ffi::from::FromFFI;
use dash_spv_ffi::types;
use dash_spv_ffi::types::LLMQValidationData;
use dash_spv_models::common::block_data::BlockData;
use dash_spv_models::common::LLMQType;
use dash_spv_models::masternode::{LLMQEntry, MasternodeEntry, MasternodeList};
use dash_spv_primitives::crypto::byte_util::{Reversable, Zeroable};
use dash_spv_primitives::crypto::data_ops::{Data, inplace_intersection};
use dash_spv_primitives::crypto::UInt256;

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
    BHL: Fn(UInt256) -> u32 + Copy,
    MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
    MND: Fn(*const types::MasternodeList) + Copy,
    AIL: Fn(UInt256) + Copy,
    SPL: Fn(LLMQType) -> bool + Copy,
    VQL: Fn(LLMQValidationData) -> bool + Copy,
> {
    pub block_height_lookup: BHL,
    pub masternode_list_lookup: MNL,
    pub masternode_list_destroy: MND,
    pub add_insight_lookup: AIL,
    pub should_process_llmq_of_type: SPL,
    pub validate_llmq_callback: VQL,
    pub use_insight_as_backup: bool,
    pub base_masternode_list_hash: Option<UInt256>,
    pub consensus_type: ConsensusType,
}

pub fn lookup_masternode_list<'a,
    MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
    MND: Fn(*const types::MasternodeList),
>(
    block_hash: UInt256,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND,
) -> Option<MasternodeList> {
    let lookup_result = masternode_list_lookup(block_hash);
    if !lookup_result.is_null() {
        let list = unsafe { (*lookup_result).decode() };
        Some(list)
    } else {
        None
    }
}

pub fn lookup_masternodes_and_quorums_for<MNL, MND>(
    block_hash: Option<UInt256>,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MND,
) -> (BTreeMap<UInt256, MasternodeEntry>, HashMap<LLMQType, HashMap<UInt256, LLMQEntry>>)
    where
        MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
        MND: Fn(*const types::MasternodeList) + Copy,
{
    if let Some(block_hash) = block_hash {
        if let Some(list) = lookup_masternode_list(block_hash, masternode_list_lookup, masternode_list_destroy) {
            return (list.masternodes, list.quorums);
        }
    }
    (BTreeMap::new(), HashMap::new())
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
                    (*modified).update_with_previous_entry(old, BlockData { height: block_height, hash: block_hash });
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

pub fn classify_quorums<'a,
    MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
    MND: Fn(*const types::MasternodeList) + Copy,
    AIL: Fn(UInt256) + Copy,
    SPL: Fn(LLMQType) -> bool + Copy,
    BHL: Fn(UInt256) -> u32 + Copy,
    VQL: Fn(LLMQValidationData) -> bool + Copy,
>(
    base_quorums: HashMap<LLMQType, HashMap<UInt256, LLMQEntry>>,
    added_quorums: HashMap<LLMQType, HashMap<UInt256, LLMQEntry>>,
    deleted_quorums: HashMap<LLMQType, Vec<UInt256>>,
    manager: Manager<BHL, MNL, MND, AIL, SPL, VQL>,
)
    -> (HashMap<LLMQType, HashMap<UInt256, LLMQEntry>>,
        HashMap<LLMQType, HashMap<UInt256, LLMQEntry>>,
        bool,
        Vec<*mut [u8; 32]>
    ) {
    let has_valid_quorums = true;
    let mut needed_masternode_lists: Vec<*mut [u8; 32]> = Vec::new();

    added_quorums
        .iter()
        .for_each(|(&llmq_type, llmqs_of_type)| {
            if (manager.should_process_llmq_of_type)(llmq_type) {
                (*llmqs_of_type).iter().for_each(|(&llmq_hash, quorum)| {
                    match lookup_masternode_list(llmq_hash, manager.masternode_list_lookup, manager.masternode_list_destroy) {
                        Some(llmq_masternode_list) =>
                            validate_quorum(
                                quorum.clone(),
                                has_valid_quorums,
                                llmq_masternode_list,
                                manager.block_height_lookup,
                                manager.validate_llmq_callback,
                                manager.consensus_type),
                        None =>
                            if (manager.block_height_lookup)(llmq_hash) != u32::MAX {
                                needed_masternode_lists.push(boxed(llmq_hash.0));
                            } else if manager.use_insight_as_backup {
                                (manager.add_insight_lookup)(llmq_hash);
                                if (manager.block_height_lookup)(llmq_hash) != u32::MAX {
                                    needed_masternode_lists.push(boxed(llmq_hash.0));
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
        .collect::<HashMap<LLMQType, HashMap<UInt256, LLMQEntry>>>());
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
fn log_quorums_map(q: HashMap<LLMQType, HashMap<UInt256, LLMQEntry>>, id: String) {
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
    BHL: Fn(UInt256) -> u32,
    VQL: Fn(LLMQValidationData) -> bool + Copy,
>(
    mut quorum: LLMQEntry,
    mut has_valid_quorums: bool,
    llmq_masternode_list: MasternodeList,
    block_height_lookup: BHL,
    validate_llmq_callback: VQL,
    consensus_type: ConsensusType,
) {
    let block_height: u32 = block_height_lookup(llmq_masternode_list.block_hash);
    let quorum_modifier = quorum.llmq_quorum_hash();
    let quorum_count = quorum.llmq_type.size();
    let valid_masternodes = if consensus_type == ConsensusType::LlmqRotation {
        valid_masternodes_for_rotated_llmq(llmq_masternode_list.masternodes, block_height)
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

pub fn valid_masternodes_for(masternodes: BTreeMap<UInt256, MasternodeEntry>, quorum_modifier: UInt256, quorum_count: u32, block_height: u32) -> Vec<MasternodeEntry> {
    let mut score_dictionary: BTreeMap<UInt256, MasternodeEntry> = masternodes.clone().into_iter().filter_map(|(_, entry)| {
        let score = MasternodeList::masternode_score(entry.clone(), quorum_modifier, block_height);
        if score.is_some() && !score.unwrap().0.is_empty() {
            Some((score.unwrap(), entry))
        } else {
            None
        }
    }).collect();

    let mut scores: Vec<UInt256> = score_dictionary.clone().into_keys().collect();
    scores.sort_by(|&s1, &s2| s2.clone().reversed().cmp(&s1.clone().reversed()));
    let masternodes_in_list_count = masternodes.len();
    let mut valid_masternodes: Vec<MasternodeEntry> = Vec::new();
    let count = min(masternodes_in_list_count, scores.len());
    for i in 0..count {
        if let Some(masternode) = score_dictionary.get_mut(&scores[i]) {
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

pub fn valid_masternodes_for_rotated_llmq(masternodes: BTreeMap<UInt256, MasternodeEntry>, block_height: u32) -> Vec<MasternodeEntry> {
    //final StoredBlock cycleQuorumBaseBlock = blockChain.getBlockStore().get(cycleQuorumBaseHeight);
    let dkg_interval = 24;
    let llmq_index = block_height % dkg_interval;
    let cycle_height = block_height - llmq_index;
    let mut valid_masternodes = Vec::new();


    valid_masternodes
}
