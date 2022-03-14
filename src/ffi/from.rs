use std::collections::{BTreeMap, HashMap};
use std::slice;
use std::time::Instant;
use crate::common::block_data::BlockData;
use crate::common::llmq_type::LLMQType;
use crate::common::socket_address::SocketAddress;
use crate::consensus::encode;
use crate::crypto::byte_util::{Reversable, UInt128, UInt160, UInt256, UInt384, UInt768};
use crate::ffi::to::ToFFI;
use crate::ffi::wrapped_types;
use crate::masternode::{masternode_entry, masternode_list, quorum_entry};
use crate::masternode::quorum_entry::QUORUM_DEFAULT_VERSION;

pub trait FromFFI<'a> {
    type Item: ToFFI<'a>;
    unsafe fn decode(&self) -> Self::Item;
}

impl<'a> FromFFI<'a> for wrapped_types::MasternodeList {
    type Item = masternode_list::MasternodeList<'a>;

    unsafe fn decode(&self) -> Self::Item {
        let t_mnl_dec = Instant::now();
        let block_hash = UInt256(*self.block_hash);
        let known_height = self.known_height;
        let masternode_merkle_root = if self.masternode_merkle_root.is_null() {
            None
        } else {
            Some(UInt256(*self.masternode_merkle_root))
        };
        let quorum_merkle_root = if self.quorum_merkle_root.is_null() {
            None
        } else {
            Some(UInt256(*self.quorum_merkle_root))
        };
        let masternodes: BTreeMap<UInt256, masternode_entry::MasternodeEntry> =
            (0..self.masternodes_count)
                .into_iter()
                .fold(BTreeMap::new(),|mut acc, i| {
                    let raw_value = *(*(self.masternodes.offset(i as isize)));
                    let value = raw_value.decode();
                    let key = value.provider_registration_transaction_hash.clone().reversed();
                    acc.insert(key, value);
                    acc
                });
        let quorums: HashMap<LLMQType, HashMap<UInt256, quorum_entry::QuorumEntry>> =
            (0..self.quorum_type_maps_count)
                .into_iter()
                .fold(HashMap::new(), |mut acc, i| {
                    let llmq_map = *(*(self.quorum_type_maps.offset(i as isize)));
                    let key = LLMQType::from(llmq_map.llmq_type);
                    let value: HashMap<UInt256, quorum_entry::QuorumEntry> =
                        (0..llmq_map.count)
                            .into_iter()
                            .fold(HashMap::new(), |mut acc, j| {
                                let raw_value = *(*(llmq_map.values.offset(j as isize)));
                                let value = raw_value.decode();
                                let key = value.quorum_hash.clone();
                                acc.insert(key, value);
                                acc
                            });
                    acc.insert(key, value);
                    acc
                });
        let unwrapped = Self::Item {
            block_hash,
            known_height,
            masternode_merkle_root,
            quorum_merkle_root,
            masternodes,
            quorums
        };
        println!("MasternodeList reflection: {:?}", Instant::now().duration_since(t_mnl_dec));
        unwrapped
    }
}

impl<'a> FromFFI<'a> for wrapped_types::MasternodeEntry {
    type Item = masternode_entry::MasternodeEntry;
    unsafe fn decode(&self) -> Self::Item {
        let provider_registration_transaction_hash = UInt256(*self.provider_registration_transaction_hash);
        let confirmed_hash = UInt256(*self.confirmed_hash);
        let confirmed_hash_hashed_with_provider_registration_transaction_hash = if self.confirmed_hash_hashed_with_provider_registration_transaction_hash.is_null() {
            None
        } else {
            Some(UInt256(*self.confirmed_hash_hashed_with_provider_registration_transaction_hash))
        };
        let ip_address = UInt128(*self.ip_address);
        let port = self.port;
        let socket_address = SocketAddress { ip_address, port };
        let operator_public_key = UInt384(*self.operator_public_key);
        let previous_operator_public_keys: BTreeMap<BlockData, UInt384> =
            (0..self.previous_operator_public_keys_count)
                .into_iter()
                .fold(BTreeMap::new(), |mut acc, i| {
                    let obj = *self.previous_operator_public_keys.offset(i as isize);
                    let key = BlockData { height: obj.block_height, hash: UInt256(obj.block_hash) };
                    let value = UInt384(obj.key);
                    acc.insert(key, value);
                    acc
                });
        let previous_masternode_entry_hashes: BTreeMap<BlockData, UInt256> =
            (0..self.previous_masternode_entry_hashes_count)
                .into_iter()
                .fold(BTreeMap::new(), |mut acc, i| {
                    let obj = *self.previous_masternode_entry_hashes.offset(i as isize);
                    let key = BlockData { height: obj.block_height, hash: UInt256(obj.block_hash) };
                    let value = UInt256(obj.hash);
                    acc.insert(key, value);
                    acc
                });
        let previous_validity: BTreeMap<BlockData, bool> =
            (0..self.previous_validity_count)
                .into_iter()
                .fold(BTreeMap::new(), |mut acc, i| {
                    let obj = *self.previous_validity.offset(i as isize);
                    let key = BlockData { height: obj.block_height, hash: UInt256(obj.block_hash) };
                    let value = obj.is_valid;
                    acc.insert(key, value);
                    acc
                });
        let update_height = self.update_height;
        let key_id_voting = UInt160(*self.key_id_voting);
        let known_confirmed_at_height = if self.known_confirmed_at_height > 0 {
            Some(self.known_confirmed_at_height)
        } else {
            None
        };
        let is_valid = self.is_valid;
        let masternode_entry_hash = UInt256(*self.masternode_entry_hash);
        masternode_entry::MasternodeEntry {
            provider_registration_transaction_hash,
            confirmed_hash,
            confirmed_hash_hashed_with_provider_registration_transaction_hash,
            socket_address,
            operator_public_key,
            previous_operator_public_keys,
            previous_masternode_entry_hashes,
            previous_validity,
            known_confirmed_at_height,
            update_height,
            key_id_voting,
            is_valid,
            masternode_entry_hash
        }
    }
}

impl<'a> FromFFI<'a> for wrapped_types::QuorumEntry {
    type Item = quorum_entry::QuorumEntry<'a>;

    unsafe fn decode(&self) -> Self::Item {
        let version = self.version;
        let quorum_hash = UInt256(*self.quorum_hash);
        let quorum_index = if self.version == QUORUM_DEFAULT_VERSION { None } else { Some(self.quorum_index) };
        let quorum_public_key = UInt384(*self.quorum_public_key);
        let quorum_threshold_signature = UInt768(*self.quorum_threshold_signature);
        let quorum_verification_vector_hash = UInt256(*self.quorum_verification_vector_hash);
        let all_commitment_aggregated_signature = UInt768(*self.all_commitment_aggregated_signature);
        let llmq_type = self.llmq_type;
        let signers_count = encode::VarInt(self.signers_count);
        let valid_members_count = encode::VarInt(self.valid_members_count);
        let signers_bitset = slice::from_raw_parts(self.signers_bitset, self.signers_bitset_length);
        let valid_members_bitset = slice::from_raw_parts(self.valid_members_bitset, self.valid_members_bitset_length);
        let length = self.length;
        let quorum_entry_hash = UInt256(*self.quorum_entry_hash);
        let verified = self.verified;
        let saved = self.saved;
        let commitment_hash = if self.commitment_hash.is_null() {
            None
        } else {
            Some(UInt256(*self.commitment_hash))
        };
        quorum_entry::QuorumEntry {
            version,
            quorum_hash,
            quorum_index,
            quorum_public_key,
            quorum_threshold_signature,
            quorum_verification_vector_hash,
            all_commitment_aggregated_signature,
            signers_count,
            llmq_type,
            valid_members_count,
            signers_bitset,
            valid_members_bitset,
            length,
            quorum_entry_hash,
            verified,
            saved,
            commitment_hash
        }
    }
}
