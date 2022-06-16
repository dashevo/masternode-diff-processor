use std::ptr::null_mut;
use dash_spv_models::common::chain_type::ChainType;
use dash_spv_primitives::crypto::byte_util::{Reversable, UInt256};
use dash_spv_primitives::hashes::hex::ToHex;
use crate::lib_tests::tests::{add_insight_lookup, block_height_lookup_5078, FFIContext, masternode_list_destroy, masternode_list_lookup, message_from_file, should_process_llmq_of_type, validate_llmq_callback};
use crate::llmq_rotation_info_process2;

#[test]
fn test_llmq_rotation() {
    let bytes = message_from_file("qrinfo--1-5078.dat".to_string());
    let length = bytes.len();
    let c_array = bytes.as_ptr();
    let merkle_root = [0u8; 32].as_ptr();
    let use_insight_as_backup= false;
    let base_masternode_list_hash = null_mut();
    let h = 5078;
    let context = &mut (FFIContext { chain: ChainType::DevNet }) as *mut _ as *mut std::ffi::c_void;
    let result = llmq_rotation_info_process2(
        c_array,
        length,
        base_masternode_list_hash,
        merkle_root,
        use_insight_as_backup,
        block_height_lookup_5078,
        masternode_list_lookup,
        masternode_list_destroy,
        add_insight_lookup,
        should_process_llmq_of_type,
        validate_llmq_callback,
        context,
    );
    println!("{:?}", result);
    let result_5078 = unsafe { *result };
    let result_at_h = unsafe { *result_5078.result_at_h };
    assert!(result_at_h.has_found_coinbase, "Did not find coinbase at height {}", h);
    // turned off on purpose as we don't have the coinbase block
    // assert!(result.valid_coinbase, "Coinbase not valid at height {}", h);
    // assert!(result_at_h.has_valid_mn_list_root, "mn list root not valid at height {}", h);
    // assert!(result_at_h.has_valid_llmq_list_root, "LLMQ list root not valid at height {}", h);
    // assert!(result_at_h.has_valid_quorums, "validQuorums not valid at height {}", h);
}
#[test]
fn test_llmq_rotation_2() {
    let bytes = message_from_file("QRINFO_1_8344.dat".to_string());
    let length = bytes.len();
    let c_array = bytes.as_ptr();
    let merkle_root = [0u8; 32].as_ptr();
    let use_insight_as_backup= false;
    let base_masternode_list_hash = null_mut();
    let context = &mut (FFIContext { chain: ChainType::DevNet }) as *mut _ as *mut std::ffi::c_void;
    println!("test_llmq_rotation_2 {:?}", bytes.to_hex());
    let result = llmq_rotation_info_process2(
        c_array,
        length,
        base_masternode_list_hash,
        merkle_root,
        use_insight_as_backup,
        block_height_lookup_,
        masternode_list_lookup,
        masternode_list_destroy,
        add_insight_lookup,
        should_process_llmq_of_type,
        validate_llmq_callback,
        context,
    );
}

unsafe extern "C" fn block_height_lookup_(block_hash: *mut [u8; 32], _context: *const std::ffi::c_void) -> u32 {
    //let bh = block_height_for(chain, masternode_list.block_hash.reversed().to_string().as_str());
    let mut h = UInt256(*(block_hash));
    let orig_s = h.clone().to_string();
    let rev = h.reversed();
    let rev_s = rev.to_string();
    match rev_s.as_str() {
        "00000072f3c73d891d86546f259ba2cd87d1aa655c447640a4257f6a8e6f7018" => 5334,
        "000000a451ba6459b3ce6128a5e8f273f9bc2010645dd4721e1b51efce18dda7" => 4207,
        "000000076aeba26f76a5d0e12e11c9b4d35d7232f1bbae6c47b4d8bef4a12b62" => 4192,
        "00000179987c39850ddd901eec6bfd0a508ec54fb6a0cd28481481aa0adf56b6" => 4168,
        "00000028bd64fd360dba79acf7cb3bae6cea18553c7232894a2ace15ada70940" => 4144,
        "000002410622902b361d1e2194f2072c6409c6f22ef5fea854d3326a27075713" => 4120,
        "000001f340d35fe89d1924de57ccbf63a7a09347835e6e4990ee2df12a4a67f9" => 4096,
        _ => u32::MAX
    }
}

#[test]
fn test_llmq_rotation_3() {
    let bytes = message_from_file("QRINFO_0125771d2f9419377aebc77e3b880afaa6f3438ccf247919ce4e9bd450a029343fe9f3a8caf3845251ee9002770cb0f2e1c6f6c43fdff480f7a59f8e29c000000001".to_string());
    let length = bytes.len();
    let c_array = bytes.as_ptr();
    let merkle_root = [0u8; 32].as_ptr();
    let use_insight_as_backup= false;
    let base_masternode_list_hash = null_mut();
    let context = &mut (FFIContext { chain: ChainType::DevNet }) as *mut _ as *mut std::ffi::c_void;
    println!("test_llmq_rotation_3 {:?}", bytes.to_hex());
    let result = llmq_rotation_info_process2(
        c_array,
        length,
        base_masternode_list_hash,
        merkle_root,
        use_insight_as_backup,
        block_height_lookup_,
        masternode_list_lookup,
        masternode_list_destroy,
        add_insight_lookup,
        should_process_llmq_of_type,
        validate_llmq_callback,
        context,
    );
}


unsafe extern "C" fn block_height_lookup_333(block_hash: *mut [u8; 32], _context: *const std::ffi::c_void) -> u32 {
    //let bh = block_height_for(chain, masternode_list.block_hash.reversed().to_string().as_str());
    let mut h = UInt256(*(block_hash));
    let orig_s = h.clone().to_string();
    let rev = h.reversed();
    let rev_s = rev.to_string();
    match rev_s.as_str() {
        "00000072f3c73d891d86546f259ba2cd87d1aa655c447640a4257f6a8e6f7018" => 5334,
        "000000a451ba6459b3ce6128a5e8f273f9bc2010645dd4721e1b51efce18dda7" => 4207,
        "000000076aeba26f76a5d0e12e11c9b4d35d7232f1bbae6c47b4d8bef4a12b62" => 4192,
        "00000179987c39850ddd901eec6bfd0a508ec54fb6a0cd28481481aa0adf56b6" => 4168,
        "00000028bd64fd360dba79acf7cb3bae6cea18553c7232894a2ace15ada70940" => 4144,
        "000002410622902b361d1e2194f2072c6409c6f22ef5fea854d3326a27075713" => 4120,
        "000001f340d35fe89d1924de57ccbf63a7a09347835e6e4990ee2df12a4a67f9" => 4096,
        _ => u32::MAX
    }
}

#[test]
fn test_devnet_333() {
    let bytes = message_from_file("QRINFO_1_19458.dat".to_string());
    let context = &mut (FFIContext { chain: ChainType::DevNet }) as *mut _ as *mut std::ffi::c_void;
    let result = llmq_rotation_info_process2(
        bytes.as_ptr(),
        bytes.len(),
        null_mut(),
        [0u8; 32].as_ptr(),
        false,
        block_height_lookup_333,
        masternode_list_lookup,
        masternode_list_destroy,
        add_insight_lookup,
        should_process_llmq_of_type,
        validate_llmq_callback,
        context
    );
}
