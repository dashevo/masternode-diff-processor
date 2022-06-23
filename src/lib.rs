#![allow(dead_code)]
#![allow(unused_variables)]
#[macro_use]
pub mod processing;
// pub mod store;

#[cfg(test)]
mod lib_tests;
#[cfg(test)]
mod tests;
mod macros;

use std::slice;
use std::ffi::c_void;
use std::ptr::null_mut;
use byte::BytesExt;
use dash_spv_ffi::ffi::boxer::{boxed, boxed_vec};
use dash_spv_ffi::ffi::callbacks::{AddInsightBlockingLookup, GetBlockHeightByHash, GetBlockHashByHeight, GetLLMQSnapshotByBlockHeight, MasternodeListDestroy, MasternodeListLookup, ShouldProcessLLMQTypeCallback, ValidateLLMQCallback};
use dash_spv_ffi::ffi::from::FromFFI;
use dash_spv_ffi::ffi::to::{encode_masternodes_map, encode_quorums_map, ToFFI};
use dash_spv_ffi::ffi::unboxer::{unbox_any, unbox_block, unbox_llmq_rotation_info, unbox_llmq_rotation_info_result, unbox_llmq_snapshot, unbox_llmq_validation_data, unbox_result};
use dash_spv_ffi::types;
use dash_spv_models::common::{LLMQType, MerkleTree};
use dash_spv_models::llmq;
use dash_spv_models::masternode::{LLMQEntry, masternode_list};
use dash_spv_primitives::consensus::encode;
use dash_spv_primitives::crypto::byte_util::{BytesDecodable, ConstDecodable, UInt256};
use crate::processing::{classify_masternodes, classify_quorums};
use crate::processing::manager::{ConsensusType, lookup_masternodes_and_quorums_for, Manager};

fn list_diff_from_ffi<'a>(list_diff: *mut types::MNListDiff) -> llmq::MNListDiff<'a> {
    unsafe { (*(list_diff)).decode() }
}

fn list_diff_result<
    BHH: Fn(UInt256) -> u32 + Copy,
    BHT: Fn(u32) -> *const u8 + Copy,
    SL: Fn(u32) -> *const types::LLMQSnapshot + Copy,
    MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
    MND: Fn(*const types::MasternodeList) + Copy,
    AI: Fn(UInt256) + Copy,
    SPQ: Fn(LLMQType) -> bool + Copy,
    VQ: Fn(types::LLMQValidationData) -> bool + Copy,
>(
    list_diff: llmq::MNListDiff,
    manager: Manager<BHH, BHT, SL, MNL, MND, AI, SPQ, VQ>,
    merkle_root: UInt256
) -> types::MNListDiffResult {
    let block_hash = list_diff.block_hash;
    let (base_masternodes,
        base_quorums) = lookup_masternodes_and_quorums_for(
        manager.base_masternode_list_hash,
        manager.masternode_list_lookup,
        manager.masternode_list_destroy);
    let block_height = list_diff.block_height;
    let coinbase_transaction = list_diff.coinbase_transaction;
    let quorums_active = coinbase_transaction.coinbase_transaction_version >= 2;
    let (added_masternodes,
        modified_masternodes,
        masternodes) = classify_masternodes(
        base_masternodes,
        list_diff.added_or_modified_masternodes,
        list_diff.deleted_masternode_hashes,
        block_height,
        block_hash
    );
    //println!("MNListDiffResult.from_diff.base_quorums: \n[{:?}] \nadded_quorums:\n [{:?}]", base_quorums.clone(), list_diff.added_quorums.clone());
    let (added_quorums,
        quorums,
        has_valid_quorums,
        needed_masternode_lists) = classify_quorums(
        base_quorums,
        list_diff.added_quorums,
        list_diff.deleted_quorums,
        manager
    );
    //println!("MNListDiffResult.from_diff.added_quorums: \n[{:?}] \nquorums:\n [{:?}]", added_quorums.clone(), quorums.clone());
    let masternode_list = masternode_list::MasternodeList::new(masternodes, quorums, block_hash, block_height, quorums_active);
    let has_valid_mn_list_root = masternode_list.has_valid_mn_list_root(&coinbase_transaction);
    let tree_element_count = list_diff.total_transactions;
    let hashes = list_diff.merkle_hashes;
    let flags = list_diff.merkle_flags;
    let has_found_coinbase = coinbase_transaction.has_found_coinbase(hashes);
    let merkle_tree = MerkleTree { tree_element_count, hashes, flags };
    let has_valid_quorum_list_root = !quorums_active || masternode_list.has_valid_llmq_list_root(&coinbase_transaction);
    let needed_masternode_lists_count = needed_masternode_lists.len();
    types::MNListDiffResult {
        block_hash: boxed(list_diff.block_hash.clone().0),
        has_found_coinbase,
        has_valid_coinbase: merkle_tree.has_root(merkle_root),
        has_valid_mn_list_root,
        has_valid_llmq_list_root: has_valid_quorum_list_root,
        has_valid_quorums,
        masternode_list: boxed(masternode_list.encode()),
        added_masternodes: encode_masternodes_map(&added_masternodes),
        added_masternodes_count: added_masternodes.len(),
        modified_masternodes: encode_masternodes_map(&modified_masternodes),
        modified_masternodes_count: modified_masternodes.len(),
        added_llmq_type_maps: encode_quorums_map(&added_quorums),
        added_llmq_type_maps_count: added_quorums.len(),
        needed_masternode_lists: boxed_vec(needed_masternode_lists),
        needed_masternode_lists_count
    }
}

pub fn mnl_diff_process<
    BHH: Fn(UInt256) -> u32 + Copy,
    BHT: Fn(u32) -> *const u8 + Copy,
    MNL: Fn(UInt256) -> *const types::MasternodeList + Copy,
>(
    message_arr: *const u8,
    message_length: usize,
    base_masternode_list_hash: *const u8,
    merkle_root: *const u8,
    use_insight_as_backup: bool,
    get_block_height_by_hash: BHH,
    get_block_hash_by_height: BHT,
    get_snapshot_by_block_height: GetLLMQSnapshotByBlockHeight,
    masternode_list_lookup: MNL,
    masternode_list_destroy: MasternodeListDestroy,
    add_insight_lookup: AddInsightBlockingLookup,
    should_process_llmq_of_type: ShouldProcessLLMQTypeCallback,
    validate_llmq_callback: ValidateLLMQCallback,
    context: *const c_void, // External Masternode Manager Diff Message Context ()
) -> *mut types::MNListDiffResult {
    println!("mnl_diff_process.start: {:?}", std::time::Instant::now());
    let message: &[u8] = unsafe { slice::from_raw_parts(message_arr, message_length as usize) };
    let desired_merkle_root = unwrap_or_failure!(UInt256::from_const(merkle_root));
    let list_diff = unwrap_or_failure!(llmq::MNListDiff::new(message, &mut 0, get_block_height_by_hash));
    let base_masternode_list_hash = if base_masternode_list_hash.is_null() { None } else { UInt256::from_const(base_masternode_list_hash) };
    let manager = Manager {
        get_block_height_by_hash,
        get_block_hash_by_height,
        masternode_list_lookup,
        masternode_list_destroy: |list: *const types::MasternodeList| unsafe { masternode_list_destroy(list) },
        add_insight_lookup: |h: UInt256| unsafe { add_insight_lookup(boxed(h.0), context) },
        should_process_llmq_of_type: |llmq_type: LLMQType| unsafe { should_process_llmq_of_type(llmq_type.into(), context) },
        validate_llmq_callback: |data: types::LLMQValidationData| unsafe { validate_llmq_callback(boxed(data), context) },
        use_insight_as_backup,
        base_masternode_list_hash,
        consensus_type: ConsensusType::LLMQ,
        get_snapshot_by_block_height: |h: u32| unsafe { get_snapshot_by_block_height(h, context) },
    };
    let result = list_diff_result(list_diff, manager, desired_merkle_root);
    println!("mnl_diff_process.finish: {:?}", std::time::Instant::now());
    boxed(result)
}

/// Core v0.17

#[no_mangle]
pub extern "C" fn mndiff_process(
    message_arr: *const u8,
    message_length: usize,
    base_masternode_list_hash: *const u8,
    merkle_root: *const u8,
    use_insight_as_backup: bool,
    get_block_height_by_hash: GetBlockHeightByHash,
    get_block_hash_by_height: GetBlockHashByHeight,
    get_snapshot_by_height: GetLLMQSnapshotByBlockHeight,
    masternode_list_lookup: MasternodeListLookup,
    masternode_list_destroy: MasternodeListDestroy,
    add_insight_lookup: AddInsightBlockingLookup,
    should_process_llmq_of_type: ShouldProcessLLMQTypeCallback,
    validate_llmq_callback: ValidateLLMQCallback,
    context: *const c_void, // External Masternode Manager Diff Message Context ()
) -> *mut types::MNListDiffResult {
    mnl_diff_process(
        message_arr,
        message_length,
        base_masternode_list_hash,
        merkle_root,
        use_insight_as_backup,
        |hash: UInt256| unsafe { get_block_height_by_hash(boxed(hash.0), context) },
        |height: u32| unsafe { get_block_hash_by_height(height, context) },
        get_snapshot_by_height,
        |hash: UInt256| unsafe { masternode_list_lookup(boxed(hash.0), context) },
        masternode_list_destroy,
        add_insight_lookup,
        should_process_llmq_of_type,
        validate_llmq_callback,
        context,
    )
}

#[no_mangle]
pub unsafe extern fn mndiff_block_hash_destroy(block_hash: *mut [u8; 32]) {
    unbox_any(block_hash);
}

#[no_mangle]
pub unsafe extern fn mndiff_quorum_validation_data_destroy(data: *mut types::LLMQValidationData) {
    unbox_llmq_validation_data(data);
}

#[no_mangle]
pub unsafe extern fn mndiff_destroy(result: *mut types::MNListDiffResult) {
    unbox_result(result);
}

/// Core v0.18

#[no_mangle]
pub extern "C" fn llmq_rotation_info_read(
    message_arr: *const u8,
    message_length: usize,
    get_block_height_by_hash: GetBlockHeightByHash,
    context: *const c_void, // External Masternode Manager Diff Message Context ()
) -> *mut types::LLMQRotationInfo {
    let message: &[u8] = unsafe { slice::from_raw_parts(message_arr, message_length as usize) };
    let bh_lookup = |h: UInt256| unsafe { get_block_height_by_hash(boxed(h.0), context) };
    let offset = &mut 0;
    let snapshot_at_h_c = boxed(unwrap_or_qr_failure!(types::LLMQSnapshot::from_bytes(message, offset)));
    let snapshot_at_h_2c = boxed(unwrap_or_qr_failure!(types::LLMQSnapshot::from_bytes(message, offset)));
    let snapshot_at_h_3c = boxed(unwrap_or_qr_failure!(types::LLMQSnapshot::from_bytes(message, offset)));
    let mn_list_diff_tip = boxed(unwrap_or_qr_failure!(llmq::MNListDiff::new(message, offset, bh_lookup)).encode());
    let mn_list_diff_at_h = boxed(unwrap_or_qr_failure!(llmq::MNListDiff::new(message, offset, bh_lookup)).encode());
    let mn_list_diff_at_h_c = boxed(unwrap_or_qr_failure!(llmq::MNListDiff::new(message, offset, bh_lookup)).encode());
    let mn_list_diff_at_h_2c = boxed(unwrap_or_qr_failure!(llmq::MNListDiff::new(message, offset, bh_lookup)).encode());
    let mn_list_diff_at_h_3c = boxed(unwrap_or_qr_failure!(llmq::MNListDiff::new(message, offset, bh_lookup)).encode());
    let extra_share = message.read_with::<bool>(offset, {}).unwrap_or(false);
    let (snapshot_at_h_4c, mn_list_diff_at_h_4c) = if extra_share {
        (boxed(unwrap_or_qr_failure!(types::LLMQSnapshot::from_bytes(message, offset))),
         boxed(unwrap_or_qr_failure!(llmq::MNListDiff::new(message, offset, bh_lookup)).encode()))
    } else {
        (null_mut(), null_mut())
    };
    let last_quorum_per_index_count = unwrap_or_qr_failure!(encode::VarInt::from_bytes(message, offset)).0 as usize;
    let mut last_quorum_per_index_vec: Vec<*mut types::LLMQEntry> = Vec::with_capacity(last_quorum_per_index_count);
    for _i in 0..last_quorum_per_index_count {
        last_quorum_per_index_vec.push(boxed(unwrap_or_qr_failure!(LLMQEntry::from_bytes(message, offset)).encode()));
    }
    let last_quorum_per_index = boxed_vec(last_quorum_per_index_vec);
    let quorum_snapshot_list_count = unwrap_or_qr_failure!(encode::VarInt::from_bytes(message, offset)).0 as usize;
    let mut quorum_snapshot_list_vec: Vec<*mut types::LLMQSnapshot> = Vec::with_capacity(quorum_snapshot_list_count);
    for _i in 0..quorum_snapshot_list_count {
        quorum_snapshot_list_vec.push(boxed(unwrap_or_qr_failure!(types::LLMQSnapshot::from_bytes(message, offset))));
    }
    let quorum_snapshot_list = boxed_vec(quorum_snapshot_list_vec);
    let mn_list_diff_list_count = unwrap_or_qr_failure!(encode::VarInt::from_bytes(message, offset)).0 as usize;
    let mut mn_list_diff_list_vec: Vec<*mut types::MNListDiff> = Vec::with_capacity(mn_list_diff_list_count);
    for _i in 0..mn_list_diff_list_count {
        mn_list_diff_list_vec.push(boxed(unwrap_or_qr_failure!(llmq::MNListDiff::new(message, offset, bh_lookup)).encode()));
    }
    let mn_list_diff_list = boxed_vec(mn_list_diff_list_vec);
    boxed(types::LLMQRotationInfo {
        snapshot_at_h_c,
        snapshot_at_h_2c,
        snapshot_at_h_3c,
        snapshot_at_h_4c,
        mn_list_diff_tip,
        mn_list_diff_at_h,
        mn_list_diff_at_h_c,
        mn_list_diff_at_h_2c,
        mn_list_diff_at_h_3c,
        mn_list_diff_at_h_4c,
        extra_share,
        last_quorum_per_index_count,
        last_quorum_per_index,
        quorum_snapshot_list_count,
        quorum_snapshot_list,
        mn_list_diff_list_count,
        mn_list_diff_list,
    })
}

#[no_mangle]
pub extern "C" fn llmq_rotation_info_process(
    info: *mut types::LLMQRotationInfo,
    base_masternode_list_hash: *const u8,
    merkle_root: *const u8,
    use_insight_as_backup: bool,
    get_block_height_by_hash: GetBlockHeightByHash,
    get_block_hash_by_height: GetBlockHashByHeight,
    get_snapshot_by_block_height: GetLLMQSnapshotByBlockHeight,
    masternode_list_lookup: MasternodeListLookup,
    masternode_list_destroy: MasternodeListDestroy,
    add_insight_lookup: AddInsightBlockingLookup,
    should_process_llmq_of_type: ShouldProcessLLMQTypeCallback,
    validate_llmq_callback: ValidateLLMQCallback,
    context: *const c_void, // External Masternode Manager Diff Message Context ()
) -> *mut types::LLMQRotationInfoResult {
    let llmq_rotation_info = unsafe { *info };
    let desired_merkle_root = unwrap_or_qr_result_failure!(UInt256::from_const(merkle_root));
    let base_masternode_list_hash = if base_masternode_list_hash.is_null() { None } else { UInt256::from_const(base_masternode_list_hash) };
    let manager = Manager {
        get_block_height_by_hash: |h: UInt256| unsafe { get_block_height_by_hash(boxed(h.0), context) },
        get_block_hash_by_height: |h: u32| unsafe { get_block_hash_by_height(h, context) },
        get_snapshot_by_block_height: |h: u32| unsafe { get_snapshot_by_block_height(h, context) },
        masternode_list_lookup: |h: UInt256| unsafe { masternode_list_lookup(boxed(h.0), context) },
        masternode_list_destroy: |list: *const types::MasternodeList| unsafe { masternode_list_destroy(list) },
        add_insight_lookup: |h: UInt256| unsafe { add_insight_lookup(boxed(h.0), context) },
        should_process_llmq_of_type: |llmq_type: LLMQType| unsafe { should_process_llmq_of_type(llmq_type.into(), context) },
        validate_llmq_callback: |data: types::LLMQValidationData| unsafe { validate_llmq_callback(boxed(data), context) },
        use_insight_as_backup,
        base_masternode_list_hash,
        consensus_type: ConsensusType::LlmqRotation,
    };
    let extra_share = llmq_rotation_info.extra_share;
    let result_at_tip = boxed(list_diff_result(list_diff_from_ffi(llmq_rotation_info.mn_list_diff_tip), manager, desired_merkle_root));
    let result_at_h = boxed(list_diff_result(list_diff_from_ffi(llmq_rotation_info.mn_list_diff_at_h), manager, desired_merkle_root ));
    let result_at_h_c = boxed(list_diff_result(list_diff_from_ffi(llmq_rotation_info.mn_list_diff_at_h_c), manager, desired_merkle_root));
    let result_at_h_2c = boxed(list_diff_result(list_diff_from_ffi(llmq_rotation_info.mn_list_diff_at_h_2c), manager, desired_merkle_root));
    let result_at_h_3c = boxed(list_diff_result(list_diff_from_ffi(llmq_rotation_info.mn_list_diff_at_h_3c), manager, desired_merkle_root));
    let result_at_h_4c = if extra_share {
        let list_diff = list_diff_from_ffi(llmq_rotation_info.mn_list_diff_at_h_4c);
        let result = list_diff_result(list_diff, manager, desired_merkle_root);
        boxed(result)
    } else {
        null_mut()
    };
    let last_quorum_per_index_count = llmq_rotation_info.last_quorum_per_index_count;
    let quorum_snapshot_list_count = llmq_rotation_info.quorum_snapshot_list_count;
    let mn_list_diff_list_count = llmq_rotation_info.mn_list_diff_list_count;
    let last_quorum_per_index = llmq_rotation_info.last_quorum_per_index;
    let mn_list_diff_list = boxed_vec((0..mn_list_diff_list_count)
        .into_iter()
        .map(|i| unsafe {
            let list_diff = (*(*llmq_rotation_info.mn_list_diff_list.offset(i as isize))).decode();
            let result = list_diff_result(list_diff, manager, desired_merkle_root);
            boxed(result)
        }).collect::<Vec<*mut types::MNListDiffResult>>());

    boxed(types::LLMQRotationInfoResult {
        result_at_tip,
        result_at_h,
        result_at_h_c,
        result_at_h_2c,
        result_at_h_3c,
        result_at_h_4c,
        snapshot_at_h_c: llmq_rotation_info.snapshot_at_h_c,
        snapshot_at_h_2c: llmq_rotation_info.snapshot_at_h_2c,
        snapshot_at_h_3c: llmq_rotation_info.snapshot_at_h_3c,
        snapshot_at_h_4c: llmq_rotation_info.snapshot_at_h_4c,
        extra_share,
        last_quorum_per_index_count,
        last_quorum_per_index,
        quorum_snapshot_list_count,
        quorum_snapshot_list: llmq_rotation_info.quorum_snapshot_list,
        mn_list_diff_list_count,
        mn_list_diff_list,
    })
}

#[no_mangle]
pub extern "C" fn llmq_rotation_info_process2(
    message_arr: *const u8,
    message_length: usize,
    base_masternode_list_hash: *const u8,
    merkle_root: *const u8,
    use_insight_as_backup: bool,
    get_block_height_by_hash: GetBlockHeightByHash,
    get_block_hash_by_height: GetBlockHashByHeight,
    get_llmq_snapshot_by_block_height: GetLLMQSnapshotByBlockHeight,
    masternode_list_lookup: MasternodeListLookup,
    masternode_list_destroy: MasternodeListDestroy,
    add_insight_lookup: AddInsightBlockingLookup,
    should_process_llmq_of_type: ShouldProcessLLMQTypeCallback,
    validate_llmq_callback: ValidateLLMQCallback,
    context: *const c_void, // External Masternode Manager Diff Message Context ()
) -> *mut types::LLMQRotationInfoResult {
    let message: &[u8] = unsafe { slice::from_raw_parts(message_arr, message_length as usize) };
    let desired_merkle_root = unwrap_or_qr_result_failure!(UInt256::from_const(merkle_root));
    let base_masternode_list_hash = if base_masternode_list_hash.is_null() { None } else { UInt256::from_const(base_masternode_list_hash) };
    let manager = Manager {
        get_block_height_by_hash: |h: UInt256| unsafe { get_block_height_by_hash(boxed(h.0), context) },
        get_block_hash_by_height: |h: u32| unsafe { get_block_hash_by_height(h, context) },
        get_snapshot_by_block_height: |h: u32| unsafe { get_llmq_snapshot_by_block_height(h, context) },
        masternode_list_lookup: |hash: UInt256| unsafe { masternode_list_lookup(boxed(hash.0), context) },
        masternode_list_destroy: |list: *const types::MasternodeList| unsafe { masternode_list_destroy(list) },
        add_insight_lookup: |hash: UInt256| unsafe { add_insight_lookup(boxed(hash.0), context) },
        should_process_llmq_of_type: |llmq_type: LLMQType| unsafe { should_process_llmq_of_type(llmq_type.into(), context) },
        validate_llmq_callback: |data: types::LLMQValidationData| unsafe { validate_llmq_callback(boxed(data), context) },
        use_insight_as_backup,
        base_masternode_list_hash,
        consensus_type: ConsensusType::LlmqRotation,
    };
    let offset = &mut 0;
    let snapshot_at_h_c = boxed(unwrap_or_qr_result_failure!(types::LLMQSnapshot::from_bytes(message, offset)));
    let snapshot_at_h_2c = boxed(unwrap_or_qr_result_failure!(types::LLMQSnapshot::from_bytes(message, offset)));
    let snapshot_at_h_3c = boxed(unwrap_or_qr_result_failure!(types::LLMQSnapshot::from_bytes(message, offset)));
    let diff_tip = unwrap_or_qr_result_failure!(llmq::MNListDiff::new(message, offset, manager.get_block_height_by_hash));
    let diff_h = unwrap_or_qr_result_failure!(llmq::MNListDiff::new(message, offset, manager.get_block_height_by_hash));
    let diff_h_c = unwrap_or_qr_result_failure!(llmq::MNListDiff::new(message, offset, manager.get_block_height_by_hash));
    let diff_h_2c = unwrap_or_qr_result_failure!(llmq::MNListDiff::new(message, offset, manager.get_block_height_by_hash));
    let diff_h_3c = unwrap_or_qr_result_failure!(llmq::MNListDiff::new(message, offset, manager.get_block_height_by_hash));
    let extra_share = message.read_with::<bool>(offset, {}).unwrap_or(false);
    let (snapshot_at_h_4c, diff_h_4c) = if extra_share {
        (boxed(unwrap_or_qr_result_failure!(types::LLMQSnapshot::from_bytes(message, offset))),
         Some(unwrap_or_qr_result_failure!(llmq::MNListDiff::new(message, offset, manager.get_block_height_by_hash))))
    } else {
        (null_mut(), None)
    };
    let last_quorum_per_index_count = unwrap_or_qr_result_failure!(encode::VarInt::from_bytes(message, offset)).0 as usize;
    let mut last_quorum_per_index_vec: Vec<*mut types::LLMQEntry> = Vec::with_capacity(last_quorum_per_index_count);
    for _i in 0..last_quorum_per_index_count {
        last_quorum_per_index_vec.push(boxed(unwrap_or_qr_result_failure!(LLMQEntry::from_bytes(message, offset)).encode()));
    }
    let last_quorum_per_index = boxed_vec(last_quorum_per_index_vec);

    let quorum_snapshot_list_count = unwrap_or_qr_result_failure!(encode::VarInt::from_bytes(message, offset)).0 as usize;
    let mut quorum_snapshot_list_vec: Vec<*mut types::LLMQSnapshot> = Vec::with_capacity(quorum_snapshot_list_count);
    for _i in 0..quorum_snapshot_list_count {
        quorum_snapshot_list_vec.push(boxed(unwrap_or_qr_result_failure!(types::LLMQSnapshot::from_bytes(message, offset))));
    }
    let quorum_snapshot_list = boxed_vec(quorum_snapshot_list_vec);

    let mn_list_diff_list_count = unwrap_or_qr_result_failure!(encode::VarInt::from_bytes(message, offset)).0 as usize;
    let mut mn_list_diff_list_vec: Vec<*mut types::MNListDiffResult> = Vec::with_capacity(mn_list_diff_list_count);
    for _i in 0..mn_list_diff_list_count {
        mn_list_diff_list_vec.push(boxed(list_diff_result(unwrap_or_qr_result_failure!(llmq::MNListDiff::new(message, offset, manager.get_block_height_by_hash)), manager, desired_merkle_root)));
    }
    let mn_list_diff_list = boxed_vec(mn_list_diff_list_vec);


    let result_at_tip = boxed(list_diff_result(diff_tip, manager, desired_merkle_root));
    let result_at_h = boxed(list_diff_result(diff_h, manager, desired_merkle_root));
    let result_at_h_c = boxed(list_diff_result(diff_h_c, manager, desired_merkle_root));
    let result_at_h_2c = boxed(list_diff_result(diff_h_2c, manager, desired_merkle_root));
    let result_at_h_3c = boxed(list_diff_result(diff_h_3c, manager, desired_merkle_root));
    let result_at_h_4c = if extra_share {
        boxed(list_diff_result(diff_h_4c.unwrap(), manager, desired_merkle_root))
    } else {
        null_mut()
    };
    boxed(types::LLMQRotationInfoResult {
        result_at_tip,
        result_at_h,
        result_at_h_c,
        result_at_h_2c,
        result_at_h_3c,
        result_at_h_4c,
        snapshot_at_h_c,
        snapshot_at_h_2c,
        snapshot_at_h_3c,
        snapshot_at_h_4c,
        extra_share,
        last_quorum_per_index,
        last_quorum_per_index_count,
        quorum_snapshot_list,
        quorum_snapshot_list_count,
        mn_list_diff_list,
        mn_list_diff_list_count
    })
}

#[no_mangle]
pub unsafe extern fn llmq_rotation_info_destroy(result: *mut types::LLMQRotationInfo) {
    unbox_llmq_rotation_info(result);
}

#[no_mangle]
pub unsafe extern fn llmq_rotation_info_result_destroy(result: *mut types::LLMQRotationInfoResult) {
    unbox_llmq_rotation_info_result(result);
}

#[no_mangle]
pub unsafe extern fn llmq_snapshot_destroy(result: *mut types::LLMQSnapshot) {
    unbox_llmq_snapshot(result);
}

#[no_mangle]
pub unsafe extern fn block_destroy(result: *mut types::Block) {
    unbox_block(result);
}


