use std::cmp::min;
use std::collections::{BTreeMap, HashMap};
use hashes::hex::ToHex;
use secrets::traits::AsContiguousBytes;
use crate::common::llmq_type::LLMQType;
use crate::consensus::Encodable;
use crate::crypto::byte_util::{merkle_root_from_hashes, Reversable, UInt256};
use crate::hashes::{Hash, sha256};
use crate::masternode::quorum_entry::QuorumEntry;
use crate::masternode::masternode_entry::MasternodeEntry;

#[repr(C)]
#[derive(Clone)]
pub struct MasternodeList<'a> {
    pub block_hash: UInt256,
    pub known_height: Option<u32>,
    pub masternode_merkle_root: Option<UInt256>,
    pub quorum_merkle_root: Option<UInt256>,
    pub masternodes: BTreeMap<UInt256, MasternodeEntry>,
    pub quorums: HashMap<LLMQType, HashMap<UInt256, QuorumEntry<'a>>>,
}

impl<'a> MasternodeList<'a> {
    pub fn new(
        masternodes: BTreeMap<UInt256, MasternodeEntry>,
        quorums: HashMap<LLMQType, HashMap<UInt256, QuorumEntry<'a>>>,
        block_hash: UInt256,
        block_height: u32
    ) -> Self {
        Self {
            quorums,
            block_hash,
            known_height: Some(block_height),
            masternode_merkle_root: None,
            quorum_merkle_root: None,
            masternodes,
        }
    }

    pub fn quorums_count(&self) -> u64 {
        let mut count: u64 = 0;
        for entry in self.quorums.values() {
            count += entry.len() as u64;
        }
        count
    }

    pub fn valid_masternodes_for(&self, quorum_modifier: UInt256, quorum_count: u32, block_height: u32) -> Vec<MasternodeEntry> {
        let score_dictionary = self.score_dictionary_for_quorum_modifier(quorum_modifier, block_height);
        // into_keys perform sorting like below
        /*NSArray *scores = [[score_dictionary allKeys] sortedArrayUsingComparator:^NSComparisonResult(id _Nonnull obj1, id _Nonnull obj2) {
            UInt256 hash1 = *(UInt256 *)((NSData *)obj1).bytes;
            UInt256 hash2 = *(UInt256 *)((NSData *)obj2).bytes;
            return uint256_sup(hash1, hash2) ? NSOrderedAscending : NSOrderedDescending;
        }];*/
        let scores: Vec<UInt256> = score_dictionary.clone().into_keys().collect();
        let mut masternodes: Vec<MasternodeEntry> = Vec::new();
        let masternodes_in_list_count = self.masternodes.len();
        let count = min(masternodes_in_list_count, scores.len());
        for i in 0..count {
            let score = scores.get(i).unwrap();
            let masternode = &score_dictionary[score];
            if masternode.is_valid_at(block_height) {
                masternodes.push(masternode.clone());
            }
            if masternodes.len() == quorum_count as usize {
                break;
            }
        }
        masternodes
    }

    pub fn score_dictionary_for_quorum_modifier(&self, quorum_modifier: UInt256, block_height: u32) -> BTreeMap<UInt256, MasternodeEntry> {
        self.masternodes.clone().into_iter().filter_map(|(_, entry)| {
            let score = self.masternode_score(entry.clone(), quorum_modifier, block_height);
            if score.is_some() && !score.unwrap().0.is_empty() {
                Some((score.unwrap(), entry))
            } else {
                None
            }
        }).collect()

    }

    pub fn masternode_score(&self, masternode_entry: MasternodeEntry, modifier: UInt256, block_height: u32) -> Option<UInt256> {
        if masternode_entry.confirmed_hash_at(block_height).is_none() {
            return None;
        }
        let mut buffer: Vec<u8> = Vec::new();
        if let Some(hash) = masternode_entry.confirmed_hash_hashed_with_provider_registration_transaction_hash_at(block_height) {
            hash.consensus_encode(&mut buffer).unwrap();
        }
        modifier.consensus_encode(&mut buffer).unwrap();
        Some(UInt256(sha256::Hash::hash(buffer.as_bytes()).into_inner()))
    }

    // pub fn provider_tx_ordered_hashes(&self) -> Vec<UInt256> {
    //     self.masternodes.into_keys().collect()
    // }

    pub fn hashes_for_merkle_root(&self, block_height: u32) -> Option<Vec<UInt256>> {
        // let pro_tx_hashes = self.provider_tx_ordered_hashes();
        // let pro_tx_hashes: Vec<UInt256> = self.masternodes.into_keys().collect();
        // let block_height = unsafe { block_height_lookup(self.block_hash.0.as_ptr()) };
        if block_height == u32::MAX {
            println!("Block height lookup queried an unknown block {:?}", self.block_hash);
            None
        } else {
            Some(self.masternodes
                .clone()
                .into_iter()
                .map(|(_, mn)| mn.simplified_masternode_entry_hash_at(block_height))
                .collect())
        }
    }

    pub fn q_merkle_root(&mut self) -> Option<UInt256> {
        if self.quorum_merkle_root.is_none() {
            let mut llmq_commitment_hashes = Vec::new();
            let c_quorums = self.quorums.clone();
            for (_q_type, quorums) in c_quorums {
                for (_q_hash, q_entry) in quorums {
                    llmq_commitment_hashes.push(q_entry.quorum_entry_hash.clone());
                }
            }
            llmq_commitment_hashes.sort_by(|h1, h2| {
                let h1rev = h1.clone().reversed();
                let h2rev = h2.clone().reversed();
                return h1rev.cmp(&h2rev);
            });
            let debug_hashes: Vec<String> = llmq_commitment_hashes.clone().into_iter().map(|h|h.0.to_hex()).collect();
            self.quorum_merkle_root = merkle_root_from_hashes(llmq_commitment_hashes);
        }
        self.quorum_merkle_root
    }

}
