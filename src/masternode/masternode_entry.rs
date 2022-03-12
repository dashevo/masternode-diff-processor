use std::collections::BTreeMap;
use byte::{BytesExt, LE};
use crate::common::block_data::BlockData;
use crate::common::socket_address::SocketAddress;
use crate::consensus::Encodable;
use crate::crypto::byte_util::{MNPayload, short_hex_string_from, UInt128, UInt160, UInt256, UInt384, Zeroable};
use crate::hashes::{Hash, sha256, sha256d};

#[derive(Clone)]
pub struct MasternodeEntry {
    pub provider_registration_transaction_hash: UInt256,
    pub confirmed_hash: UInt256,
    pub confirmed_hash_hashed_with_provider_registration_transaction_hash: Option<UInt256>,
    pub socket_address: SocketAddress,
    pub operator_public_key: UInt384,
    pub previous_operator_public_keys: BTreeMap<BlockData, UInt384>,
    pub previous_masternode_entry_hashes: BTreeMap<BlockData, UInt256>,
    pub previous_validity: BTreeMap<BlockData, bool>,
    pub known_confirmed_at_height: Option<u32>,
    pub update_height: u32,
    pub key_id_voting: UInt160,
    pub is_valid: bool,
    pub masternode_entry_hash: UInt256,
}
impl std::fmt::Debug for MasternodeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MasternodeEntry")
            .field("provider_registration_transaction_hash", &self.provider_registration_transaction_hash)
            // .field("update_height", &self.update_height)
            // .field("masternode_entry_hash", &self.masternode_entry_hash)
            // .field("previous_masternode_entry_hashes", &self.previous_masternode_entry_hashes)
            .finish()
    }
}

impl MasternodeEntry {
    fn calculate_masternode_entry_hash(
        provider_registration_transaction_hash: UInt256,
        confirmed_hash: UInt256,
        socket_address: SocketAddress,
        operator_public_key: UInt384,
        key_id_voting: UInt160,
        is_valid: u8,
    ) -> UInt256 {
        let offset: &mut usize = &mut 0;
        const HASH_IMPORTANT_DATA_LENGTH: usize = 32 + 32 + 16 + 2 + 48 + 20 + 1;
        let mut buffer: Vec<u8> = Vec::with_capacity(HASH_IMPORTANT_DATA_LENGTH);
        *offset += provider_registration_transaction_hash.consensus_encode(&mut buffer).unwrap();
        *offset += confirmed_hash.consensus_encode(&mut buffer).unwrap();
        *offset += socket_address.ip_address.consensus_encode(&mut buffer).unwrap();
        *offset += socket_address.port.swap_bytes().consensus_encode(&mut buffer).unwrap();
        *offset += operator_public_key.consensus_encode(&mut buffer).unwrap();
        *offset += key_id_voting.consensus_encode(&mut buffer).unwrap();
        *offset += is_valid.consensus_encode(&mut buffer).unwrap();
        UInt256(sha256d::Hash::hash(&buffer).into_inner())
    }

    pub fn confirmed_hash_at(&self, block_height: u32) -> Option<UInt256> {
        match self.known_confirmed_at_height {
            Some(h) => if h > block_height { None } else { Some(self.confirmed_hash) },
            None => None
        }
        // if self.known_confirmed_at_height.is_some() &&
        //     self.known_confirmed_at_height.unwrap() > block_height {
        //     None
        // } else {
        //     Some(self.confirmed_hash)
        // }
        // if (!self.knownConfirmedAtHeight) return self.confirmedHash;
        // if (self.knownConfirmedAtHeight > blockHeight) {
        //     return UINT256_ZERO;
        // } else {
        //     return self.confirmedHash;
        // }
    }

    pub fn confirmed_hash_hashed_with_provider_registration_transaction_hash_at(&self, block_height: u32) -> Option<UInt256> {
        if self.known_confirmed_at_height.is_none() ||
            self.known_confirmed_at_height? <= block_height {
            self.confirmed_hash_hashed_with_provider_registration_transaction_hash
        } else {
            Some(MasternodeEntry::hash_confirmed_hash(UInt256::default(), self.provider_registration_transaction_hash))
        }
    }

    pub fn host(&self) -> String {
        let ip = self.socket_address.ip_address;
        let port = self.socket_address.port;
        format!("{}:{}", ip.to_string(), port.to_string())
    }

    pub fn payload_data(&self) -> UInt256 {
        MasternodeEntry::calculate_masternode_entry_hash(
            self.provider_registration_transaction_hash,
            self.confirmed_hash,
            self.socket_address,
            self.operator_public_key,
            self.key_id_voting,
            if self.is_valid {1} else {0}
        )
    }

    pub fn hash_confirmed_hash(confirmed_hash: UInt256, pro_reg_tx_hash: UInt256) -> UInt256 {
        let mut buffer: Vec<u8> = Vec::with_capacity(64);
        let offset: &mut usize = &mut 0;
        *offset += pro_reg_tx_hash.consensus_encode(&mut buffer).unwrap();
        *offset += confirmed_hash.consensus_encode(&mut buffer).unwrap();
        UInt256(sha256::Hash::hash(&buffer).into_inner())
    }

    pub fn is_valid_at(&self, block_height: u32) -> bool {
        if self.previous_validity.len() == 0 || block_height == u32::MAX {
            return self.is_valid;
        }
        let mut min_distance = u32::MAX;
        let mut is_valid = self.is_valid;
        for (BlockData { height, .. }, validity) in self.previous_validity.clone() {
            if height <= block_height {
                continue;
            }
            let distance = height - block_height;
            if distance < min_distance {
                min_distance = distance;
                is_valid = validity;
            }
        }
        is_valid
    }

    pub fn operator_public_key_at(&self, block_height: u32) -> UInt384 {
        if self.previous_operator_public_keys.is_empty() {
            return self.operator_public_key;
        }
        let mut min_distance = u32::MAX;
        let mut used_previous_operator_public_key_at_block_hash = self.operator_public_key;
        for (BlockData{height,..}, key) in self.previous_operator_public_keys.clone() {
            if height <= block_height {
                continue;
            }
            let distance = height - block_height;
            if distance < min_distance {
                min_distance = distance;
                used_previous_operator_public_key_at_block_hash = key;
            }
        }
        used_previous_operator_public_key_at_block_hash
    }

    pub fn masternode_entry_hash_at(&self, block_height: u32) -> UInt256 {
        if self.previous_masternode_entry_hashes.len() == 0 ||
            block_height == u32::MAX {
            return self.masternode_entry_hash.clone();
        }
        let hashes: BTreeMap<BlockData, UInt256> = self.previous_masternode_entry_hashes.clone();
        let mut min_distance = u32::MAX;
        let mut used_hash = self.masternode_entry_hash.clone();
        for (BlockData { height, .. }, hash) in hashes {
            if height <= block_height {
                continue;
            }
            let distance = height - block_height;
            if distance < min_distance {
                min_distance = distance;
                //println!("SME Hash for proTxHash {:?} : Using {:?} instead of {:?} for list at block height {}", self.provider_registration_transaction_hash, hash, used_hash, block_height);
                used_hash = hash;
            }
        }
        used_hash
    }


    pub fn unique_id(&self) -> String {
        short_hex_string_from(&self.provider_registration_transaction_hash.0)
    }

    pub fn new(message: MNPayload, block_height: u32) -> Option<MasternodeEntry> {
        // let length = message.len();
        let message = message.0;
        let offset = &mut 0;
        let provider_registration_transaction_hash = match message.read_with::<UInt256>(offset, LE) {
            Ok(data) => data,
            Err(_err) => { return None; }
        };
        let confirmed_hash = match message.read_with::<UInt256>(offset, LE) {
            Ok(data) => data,
            Err(_err) => { return None; }
        };
        let known_confirmed_at_height: Option<u32> =
            if !confirmed_hash.is_zero() &&
                block_height != u32::MAX {
                Some(block_height)
            } else {
                None
            };
        let ip_address = match message.read_with::<UInt128>(offset, LE) {
            Ok(data) => data,
            Err(_err) => { return None; }
        };
        let port = match message.read_with::<u16>(offset, LE) {
            Ok(data) => data.swap_bytes(),
            Err(_err) => { return None; }
        };
        let socket_address = SocketAddress { ip_address, port };
        let operator_public_key = match message.read_with::<UInt384>(offset, LE) {
            Ok(data) => data,
            Err(_err) => { return None; }
        };
        let key_id_voting = match message.read_with::<UInt160>(offset, LE) {
            Ok(data) => data,
            Err(_err) => { return None; }
        };
        let is_valid = match message.read_with::<u8>(offset, LE) {
            Ok(data) => data,
            Err(_err) => 0
        };
        let masternode_entry_hash = MasternodeEntry::calculate_masternode_entry_hash(
            provider_registration_transaction_hash,
            confirmed_hash,
            socket_address,
            operator_public_key,
            key_id_voting,
            is_valid
        );
        Some(MasternodeEntry {
            provider_registration_transaction_hash,
            confirmed_hash,
            confirmed_hash_hashed_with_provider_registration_transaction_hash: None,
            socket_address,
            operator_public_key,
            previous_operator_public_keys: BTreeMap::new(),
            previous_masternode_entry_hashes: BTreeMap::new(),
            previous_validity: BTreeMap::new(),
            known_confirmed_at_height,
            update_height: block_height,
            key_id_voting,
            is_valid: is_valid != 0,
            masternode_entry_hash
        })
    }
}
