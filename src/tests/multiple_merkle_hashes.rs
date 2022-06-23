use dash_spv_models::common::merkle_tree::MerkleTree;
use dash_spv_primitives::crypto::byte_util::UInt256;
use dash_spv_primitives::hashes::hex::{FromHex, ToHex};

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
