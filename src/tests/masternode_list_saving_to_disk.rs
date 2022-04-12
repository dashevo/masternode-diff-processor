use std::ptr::null_mut;
use bitcoin_hashes::hex::FromHex;
use crate::common::chain_type::ChainType;
use crate::lib_tests::tests::{add_insight_lookup, assert_diff_result, FFIContext, masternode_list_destroy, message_from_file, should_process_llmq_of_type, validate_llmq_callback};
use crate::{FromFFI, mnl_diff_process, UInt256};

#[test]
fn test_mnl_saving_to_disk() { // testMNLSavingToDisk
    let chain = ChainType::TestNet;
    let bytes = message_from_file("ML_at_122088.dat".to_string());
    let context = &mut (FFIContext { chain }) as *mut _ as *mut std::ffi::c_void;
    let result = mnl_diff_process(
        bytes.as_ptr(),
        bytes.len(),
        null_mut(),
        [0u8; 32].as_ptr(),
        false,
        |block_hash| 122088,
        |block_hash| null_mut(),
        masternode_list_destroy,
        add_insight_lookup,
        should_process_llmq_of_type,
        validate_llmq_callback,
        context
    );
    println!("{:?}", result);
    let result = unsafe { *result };
    let block_hash = UInt256(unsafe { *result.block_hash });
    let masternode_list = unsafe { *result.masternode_list };
    let masternode_list_decoded = unsafe { masternode_list.decode() };
    assert_eq!(
        UInt256::from_hex("94d0af97187af3b9311c98b1cf40c9c9849df0af55dc63b097b80d4cf6c816c5").unwrap(),
        masternode_list_decoded.masternode_merkle_root.unwrap(),
        "MNList merkle root should be valid");
    assert_diff_result(chain, result);
}