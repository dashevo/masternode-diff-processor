use crate::ffi::wrapped_types::{LLMQMap, MasternodeEntry, MasternodeList, MndiffResult, QuorumEntry, QuorumValidationData};

pub unsafe fn unbox_any<T: ?Sized>(any: *mut T) -> Box<T> {
    Box::from_raw(any)
}

pub unsafe fn unbox_vec<T>(vec: Vec<*mut T>) -> Vec<Box<T>> {
    vec.iter().map(|&x| unbox_any(x)).collect()
}

pub unsafe fn unbox_vec_ptr<T>(ptr: *mut T, count: usize) -> Vec<T> {
    Vec::from_raw_parts(ptr, count, count)
}

pub unsafe fn unbox_masternode_entry(x: *mut MasternodeEntry) {
    let entry = unbox_any(x);
    unbox_any(entry.confirmed_hash);
    if !entry.confirmed_hash_hashed_with_provider_registration_transaction_hash.is_null() {
        unbox_any(entry.confirmed_hash_hashed_with_provider_registration_transaction_hash);
    }
    unbox_any(entry.key_id_voting);
    unbox_any(entry.masternode_entry_hash);
    unbox_any(entry.operator_public_key);
    unbox_vec_ptr(entry.previous_masternode_entry_hashes, entry.previous_masternode_entry_hashes_count);
    unbox_vec_ptr(entry.previous_operator_public_keys, entry.previous_operator_public_keys_count);
    unbox_vec_ptr(entry.previous_validity, entry.previous_validity_count);
    unbox_any(entry.provider_registration_transaction_hash);
    unbox_any(entry.ip_address);
}

pub unsafe fn unbox_quorum_entry(x: *mut QuorumEntry) {
    let entry = unbox_any(x);
    unbox_any(entry.all_commitment_aggregated_signature);
    if !entry.commitment_hash.is_null() {
        unbox_any(entry.commitment_hash);
    }

    unbox_any(entry.quorum_entry_hash);
    unbox_any(entry.quorum_hash);
    unbox_any(entry.quorum_public_key);
    unbox_any(entry.quorum_threshold_signature);
    unbox_any(entry.quorum_verification_vector_hash);
    let signers_bitset = std::ptr::slice_from_raw_parts_mut(entry.signers_bitset, entry.signers_bitset_length);
    let valid_members_bitset = std::ptr::slice_from_raw_parts_mut(entry.valid_members_bitset, entry.valid_members_bitset_length);
    unbox_any(signers_bitset as *mut [u8]);
    unbox_any(valid_members_bitset as *mut [u8]);
}

pub unsafe fn unbox_llmq_map(x: *mut LLMQMap) {
    let entry = unbox_any(x);
    let values = unbox_vec_ptr(entry.values, entry.count);
    for &x in values.iter() {
        unbox_quorum_entry(x);
    }
}
pub unsafe fn unbox_masternode_list(masternode_list: Box<MasternodeList>) {
    unbox_any(masternode_list.block_hash);
    if !masternode_list.masternode_merkle_root.is_null() {
        unbox_any(masternode_list.masternode_merkle_root);
    }
    if !masternode_list.quorum_merkle_root.is_null() {
        unbox_any(masternode_list.quorum_merkle_root);
    }
    unbox_masternode_vec(unbox_vec_ptr(masternode_list.masternodes, masternode_list.masternodes_count));
    unbox_llmq_map_vec(unbox_vec_ptr(masternode_list.quorum_type_maps, masternode_list.quorum_type_maps_count));
}

pub unsafe fn unbox_masternode_vec(vec: Vec<*mut MasternodeEntry>) {
    for &x in vec.iter() {
        unbox_masternode_entry(x);
    }
}

pub unsafe fn unbox_llmq_map_vec(vec: Vec<*mut LLMQMap>) {
    for &x in vec.iter() {
        unbox_llmq_map(x);
    }
}

pub unsafe fn unbox_quorum_validation_data(quorum_validation_data: *mut QuorumValidationData) {
    let result = unbox_any(quorum_validation_data);
    unbox_any(result.all_commitment_aggregated_signature);
    unbox_any(result.commitment_hash);
    unbox_any(result.quorum_public_key);
    unbox_any(result.quorum_threshold_signature);
    unbox_vec(unbox_vec_ptr(result.items, result.count));
}

pub unsafe fn unbox_result(result: *mut MndiffResult) {
    let res = unbox_any(result);
    unbox_any(res.block_hash);
    unbox_masternode_list(unbox_any(res.masternode_list));
    unbox_vec(unbox_vec_ptr(res.needed_masternode_lists, res.needed_masternode_lists_count));
    unbox_masternode_vec(unbox_vec_ptr(res.added_masternodes, res.added_masternodes_count));
    unbox_masternode_vec(unbox_vec_ptr(res.modified_masternodes, res.modified_masternodes_count));
    unbox_llmq_map_vec(unbox_vec_ptr(res.added_quorum_type_maps, res.added_quorum_type_maps_count));
}
