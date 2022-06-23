use std::collections::BTreeMap;
use std::ptr::null_mut;
use dash_spv_ffi::ffi::from::FromFFI;
use dash_spv_ffi::ffi::to::ToFFI;
use dash_spv_ffi::types;
use dash_spv_models::common::chain_type::ChainType;
use dash_spv_models::masternode::LLMQEntry;
use dash_spv_primitives::crypto::byte_util::{Reversable, UInt256};
use dash_spv_primitives::hashes::hex::ToHex;
use crate::{LLMQType, mnl_diff_process};
use crate::lib_tests::tests::{add_insight_lookup, assert_diff_result, block_height_for, FFIContext, get_llmq_snapshot_by_block_height, masternode_list_destroy, message_from_file, should_process_llmq_of_type, validate_llmq_callback};

#[test]
fn testnet_llmq_verification() { //testTestnetQuorumVerification
    let bytes = message_from_file("MNL_0_122928.dat".to_string());
    let merkle_root = [0u8; 32].as_ptr();
    let use_insight_as_backup= false;
    let chain = ChainType::TestNet;
    let get_block_height_by_hash = |block_hash: UInt256| block_height_for(chain, block_hash.clone().reversed().0.to_hex().as_str());
    let base_masternode_list_hash: *const u8 = null_mut();
    let context = &mut FFIContext { chain } as *mut _ as *mut std::ffi::c_void;

    let result = mnl_diff_process(
        bytes.as_ptr(),
        bytes.len(),
        base_masternode_list_hash,
        merkle_root,
        use_insight_as_backup,
        get_block_height_by_hash,
        |height| null_mut(),
        get_llmq_snapshot_by_block_height,
        |block_hash| null_mut(),
        masternode_list_destroy,
        add_insight_lookup,
        should_process_llmq_of_type,
        validate_llmq_callback,
        context
    );
    println!("{:?}", result);
    let result_119064 = unsafe { *result };
    assert_diff_result(chain, result_119064);
    let is_valid = result_119064.is_valid();
    println!("is_valid: {}", is_valid);
    if is_valid {
        let bytes = message_from_file("MNL_122928_123000.dat".to_string());
        let block_hash_119064 = UInt256(unsafe { *result_119064.block_hash });
        let masternode_list_119064 = unsafe { *result_119064.masternode_list };
        let masternode_list_119064_decoded = unsafe { masternode_list_119064.decode() };
        let masternode_list_119064_encoded = masternode_list_119064_decoded.encode();
        let result = mnl_diff_process(
            bytes.as_ptr(),
            bytes.len(),
            block_hash_119064.0.as_ptr(),
            merkle_root,
            use_insight_as_backup,
            get_block_height_by_hash,
            |height| null_mut(),
            get_llmq_snapshot_by_block_height,
            |hash|
                if hash == block_hash_119064 {
                    &masternode_list_119064_encoded as *const types::MasternodeList
                } else {
                    null_mut()
                }
            ,
            masternode_list_destroy,
            add_insight_lookup,
            should_process_llmq_of_type,
            validate_llmq_callback,
            context
        );
        println!("{:?}", result);
        let result_119200 = unsafe { *result };
        assert_diff_result(chain, result_119200);
        let masternode_list_119200 = unsafe { *result_119200.masternode_list };
        let masternode_list_119200_decoded = unsafe { masternode_list_119200.decode() };
        let added_quorums = (0..result_119200.added_llmq_type_maps_count)
            .into_iter()
            .fold(BTreeMap::new(), |mut acc, i| unsafe {
                let map = *(*(result_119200.added_llmq_type_maps.offset(i as isize)));
                let llmq_type = LLMQType::from(map.llmq_type);
                let entry_map = (0..map.count)
                    .into_iter()
                    .fold(BTreeMap::new(), |mut hacc, j| {
                        let raw_entry = *(*(map.values.offset(j as isize)));
                        let entry = raw_entry.decode();
                        hacc.insert(entry.llmq_hash, entry);
                        hacc
                    });
                acc.insert(llmq_type, entry_map);
                acc
            });
        let hmm: BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>> = added_quorums.into_iter().filter(|(_, map)| map.contains_key(&block_hash_119064)).collect();
        assert!(hmm.len() > 0, "There should be a quorum using 119064");
        // assert!(added_quorums.contains_key(&block_hash_119064), "There should be a quorum using 119064");
        // TODO: verify with QuorumValidationData (need implement BLS before)
        //let quorum_to_verify = added_quorums[&block_hash_119064];
        //quorum_to_verify.validate_with_masternode_list(masternode_list_119064_decoded);
        //assert!(quorum_to_verify.verified, "Unable to verify quorum");
    }
}
