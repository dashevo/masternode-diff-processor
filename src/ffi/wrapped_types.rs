use std::ffi::c_void;
use std::ptr::null_mut;
use crate::common::llmq_type::LLMQType;

/// This types reflected for FFI
#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct MasternodeList {
    pub block_hash: *mut [u8; 32],
    pub known_height: u32,
    pub masternode_merkle_root: *mut [u8; 32], // nullable
    pub quorum_merkle_root: *mut [u8; 32], // nullable
    pub masternodes: *mut *mut MasternodeEntry,
    pub masternodes_count: usize,
    pub quorum_type_maps: *mut *mut LLMQMap,
    pub quorum_type_maps_count: usize,
}

#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct LLMQMap {
    pub llmq_type: u8,
    pub values: *mut *mut QuorumEntry,
    pub count: usize,
}

#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct MasternodeEntry {
    pub confirmed_hash: *mut [u8; 32],
    pub confirmed_hash_hashed_with_provider_registration_transaction_hash: *mut [u8; 32], // nullable
    pub is_valid: bool,
    pub key_id_voting: *mut [u8; 20],
    pub known_confirmed_at_height: u32, // nullable
    pub masternode_entry_hash: *mut [u8; 32],
    pub operator_public_key: *mut [u8; 48],
    pub previous_masternode_entry_hashes: *mut MasternodeEntryHash,
    pub previous_masternode_entry_hashes_count: usize,
    pub previous_operator_public_keys: *mut OperatorPublicKey,
    pub previous_operator_public_keys_count: usize,
    pub previous_validity: *mut Validity,
    pub previous_validity_count: usize,
    pub provider_registration_transaction_hash: *mut [u8; 32],
    pub ip_address: *mut [u8; 16],
    pub port: u16,
    pub update_height: u32,
}

#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct QuorumEntry {
    pub all_commitment_aggregated_signature: *mut [u8; 96],
    pub commitment_hash: *mut [u8; 32], // nullable
    pub length: usize,
    pub llmq_type: LLMQType,
    pub quorum_entry_hash: *mut [u8; 32],
    pub quorum_hash: *mut [u8; 32],
    pub quorum_index: u32,
    pub quorum_public_key: *mut [u8; 48],
    pub quorum_threshold_signature: *mut [u8; 96],
    pub quorum_verification_vector_hash: *mut [u8; 32],
    pub saved: bool,
    pub signers_bitset: *mut u8,
    pub signers_bitset_length: usize,
    pub signers_count: u64,
    pub valid_members_bitset: *mut u8,
    pub valid_members_bitset_length: usize,
    pub valid_members_count: u64,
    pub verified: bool,
    pub version: u16,
}
#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct VarInt { // 9 // 72
    pub value: u64,
    pub length: usize
}
#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct Validity { // 37 // 296
    pub block_hash: [u8; 32],
    pub block_height: u32,
    pub is_valid: bool,
}
#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct OperatorPublicKey { // 84 // 692
    pub block_hash: [u8; 32],
    pub block_height: u32,
    pub key: [u8; 48],
}
#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct MasternodeEntryHash {
    pub block_hash: [u8; 32],
    pub block_height: u32,
    pub hash: [u8; 32],
}

#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct MndiffResult {
    pub block_hash: *mut [u8; 32],
    pub has_found_coinbase: bool, //1 byte
    pub has_valid_coinbase: bool, //1 byte
    pub has_valid_mn_list_root: bool, //1 byte
    pub has_valid_quorum_list_root: bool, //1 byte
    pub has_valid_quorums: bool, //1 byte
    pub masternode_list: *mut MasternodeList,
    pub added_masternodes: *mut *mut MasternodeEntry,
    pub added_masternodes_count: usize,
    pub modified_masternodes: *mut *mut MasternodeEntry,
    pub modified_masternodes_count: usize,
    pub added_quorum_type_maps: *mut *mut LLMQMap,
    pub added_quorum_type_maps_count: usize,
    pub needed_masternode_lists:  *mut *mut [u8; 32], // [u8; 32]
    pub needed_masternode_lists_count: usize,
}
#[repr(C)] #[derive(Clone, Copy, Debug)]
pub struct QuorumValidationData {
    pub items: *mut *mut [u8; 48],
    pub count: usize,
    pub commitment_hash: *mut [u8; 32],
    pub all_commitment_aggregated_signature: *mut [u8; 96],
    pub quorum_threshold_signature: *mut [u8; 96],
    pub quorum_public_key: *mut [u8; 48],
}

pub type AddInsightBlockingLookup = unsafe extern "C" fn(block_hash: *mut [u8; 32], context: *const c_void);
pub type ShouldProcessQuorumTypeCallback = unsafe extern "C" fn(quorum_type: u8, context: *const c_void) -> bool;
pub type ValidateQuorumCallback = unsafe extern "C" fn(data: *mut QuorumValidationData, context: *const c_void) -> bool;

pub type BlockHeightLookup = unsafe extern "C" fn(block_hash: *mut [u8; 32], context: *const c_void) -> u32;
pub type MasternodeListLookup = unsafe extern "C" fn(block_hash: *mut [u8; 32], context: *const c_void) -> *const MasternodeList;
pub type MasternodeListDestroy = unsafe extern "C" fn(*const MasternodeList);

impl Default for MndiffResult {
    fn default() -> Self {
        MndiffResult {
            block_hash: null_mut(),
            has_found_coinbase: false,
            has_valid_coinbase: false,
            has_valid_mn_list_root: false,
            has_valid_quorum_list_root: false,
            has_valid_quorums: false,
            masternode_list: null_mut(),
            added_masternodes: null_mut(),
            added_masternodes_count: 0,
            modified_masternodes: null_mut(),
            modified_masternodes_count: 0,
            added_quorum_type_maps: null_mut(),
            added_quorum_type_maps_count: 0,
            needed_masternode_lists: null_mut(),
            needed_masternode_lists_count: 0
        }
    }
}
