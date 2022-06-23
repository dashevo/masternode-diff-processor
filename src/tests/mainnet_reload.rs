use dash_spv_models::common::chain_type::ChainType;
use dash_spv_primitives::crypto::byte_util::{Reversable, UInt256};
use dash_spv_primitives::hashes::hex::ToHex;
use crate::lib_tests::tests::{block_height_for, load_masternode_lists_for_files};

#[test]
fn test_mainnet_reload() {
    let chain = ChainType::MainNet;
    let files = vec![
        "MNL_0_1090944.dat".to_string(),
        "MNL_1090944_1091520.dat".to_string(),
        "MNL_1091520_1091808.dat".to_string(),
        "MNL_1091808_1092096.dat".to_string(),
        "MNL_1092096_1092336.dat".to_string(),
        "MNL_1092336_1092360.dat".to_string(),
        "MNL_1092360_1092384.dat".to_string(),
        "MNL_1092384_1092408.dat".to_string(),
        "MNL_1092408_1092432.dat".to_string(),
        "MNL_1092432_1092456.dat".to_string(),
        "MNL_1092456_1092480.dat".to_string(),
        "MNL_1092480_1092504.dat".to_string(),
        "MNL_1092504_1092528.dat".to_string(),
        "MNL_1092528_1092552.dat".to_string(),
        "MNL_1092552_1092576.dat".to_string(),
        "MNL_1092576_1092600.dat".to_string(),
        "MNL_1092600_1092624.dat".to_string(),
        "MNL_1092624_1092648.dat".to_string(),
        "MNL_1092648_1092672.dat".to_string(),
        "MNL_1092672_1092696.dat".to_string(),
        "MNL_1092696_1092720.dat".to_string(),
        "MNL_1092720_1092744.dat".to_string(),
        "MNL_1092744_1092768.dat".to_string(),
        "MNL_1092768_1092792.dat".to_string(),
        "MNL_1092792_1092816.dat".to_string(),
        "MNL_1092816_1092840.dat".to_string(),
        "MNL_1092840_1092864.dat".to_string(),
        "MNL_1092864_1092888.dat".to_string(),
        "MNL_1092888_1092916.dat".to_string(),
    ];
    let block_height_lookup = |block_hash: UInt256| block_height_for(chain, block_hash.clone().reversed().0.to_hex().as_str());
    let (success, lists) = load_masternode_lists_for_files(files, chain, block_height_lookup);
    assert!(success, "Unsuccessful");
    assert_eq!(lists.len(), 29, "There should be 29 masternode lists");
}
