use std::cmp::min;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;
use crate::common::llmq_type::LLMQType;
use crate::consensus::Encodable;
use crate::crypto::byte_util::{merkle_root_from_hashes, Reversable, UInt256};
use crate::hashes::{Hash, sha256};
use crate::masternode::quorum_entry::QuorumEntry;
use crate::masternode::masternode_entry::MasternodeEntry;
use crate::Zeroable;

#[derive(Clone)]
pub struct MasternodeList<'a> {
    pub block_hash: UInt256,
    pub known_height: u32,
    pub masternode_merkle_root: Option<UInt256>,
    pub quorum_merkle_root: Option<UInt256>,
    pub masternodes: BTreeMap<UInt256, MasternodeEntry>,
    pub quorums: HashMap<LLMQType, HashMap<UInt256, QuorumEntry<'a>>>,
}

impl<'a> std::fmt::Debug for MasternodeList<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MasternodeList")
            .field("block_hash", &self.block_hash)
            .field("known_height", &self.known_height)
            .field("masternode_merkle_root", &self.masternode_merkle_root)
            .field("quorum_merkle_root", &self.quorum_merkle_root)
            .field("masternodes", &self.masternodes.len())
            .field("quorums", &self.quorums.len())
            .finish()
    }
}

impl<'a> MasternodeList<'a> {
    pub fn new(
        masternodes: BTreeMap<UInt256, MasternodeEntry>,
        quorums: HashMap<LLMQType, HashMap<UInt256, QuorumEntry<'a>>>,
        block_hash: UInt256,
        block_height: u32,
        quorums_active: bool
    ) -> Self {
        let mut list = Self {
            quorums,
            block_hash,
            known_height: block_height,
            masternode_merkle_root: None,
            quorum_merkle_root: None,
            masternodes,
        };
        if let Some(hashes) = list.hashes_for_merkle_root(block_height) {
            let t0 = Instant::now();
            list.masternode_merkle_root = merkle_root_from_hashes(hashes);
            println!("mndiff_process. masternode_merkle_root: {:?}", Instant::now().duration_since(t0));
        }
        if quorums_active {
            let hashes = list.hashes_for_quorum_merkle_root();
            let t0 = Instant::now();
            list.quorum_merkle_root = merkle_root_from_hashes(hashes);
            println!("mndiff_process. quorum_merkle_root: {:?}", Instant::now().duration_since(t0));
        }
        list
    }

    pub fn quorums_count(&self) -> u64 {
        let mut count: u64 = 0;
        for entry in self.quorums.values() {
            count += entry.len() as u64;
        }
        count
    }

    pub fn valid_masternodes_for(&self, quorum_modifier: UInt256, quorum_count: u32, block_height: u32) -> Vec<MasternodeEntry> {
        let mut score_dictionary: BTreeMap<UInt256, MasternodeEntry> = self.masternodes
            .clone()
            .into_iter()
            .filter_map(|(h, entry)| match MasternodeList::masternode_score(entry.clone(), quorum_modifier, block_height) {
                Some(score) => if score.is_zero() { None } else { Some((score, entry)) },
                None => None
            })
            .collect();
        let mut scores: Vec<UInt256> = score_dictionary.clone().into_keys().collect();
        scores.sort_by(|&s1, &s2| s2.clone().reversed().cmp(&s1.clone().reversed()));
        let mut masternodes: Vec<MasternodeEntry> = Vec::new();
        let masternodes_in_list_count = self.masternodes.len();
        let count = min(masternodes_in_list_count, scores.len());
        for i in 0..count {
            if let Some(masternode) = score_dictionary.get_mut(&scores[i]) {
                if (*masternode).is_valid_at(block_height) {
                    masternodes.push((*masternode).clone());
                }
            }
            if masternodes.len() == quorum_count as usize {
                break;
            }
        }
        masternodes
    }

    pub fn masternode_score(masternode_entry: MasternodeEntry, modifier: UInt256, block_height: u32) -> Option<UInt256> {
        if masternode_entry.confirmed_hash_at(block_height).is_none() {
            return None;
        }
        let mut buffer: Vec<u8> = Vec::new();
        if let Some(hash) = masternode_entry.confirmed_hash_hashed_with_provider_registration_transaction_hash_at(block_height) {
            hash.consensus_encode(&mut buffer).unwrap();
        }
        modifier.consensus_encode(&mut buffer).unwrap();
        Some(UInt256(sha256::Hash::hash(&buffer).into_inner()))
    }

    pub fn hashes_for_merkle_root(&self, block_height: u32) -> Option<Vec<UInt256>> {
        let t0 = Instant::now();
        if block_height == u32::MAX {
            println!("Block height lookup queried an unknown block {:?}", self.block_hash);
            None
        } else {
            let t1 = Instant::now();
            let mut pro_tx_hashes: Vec<UInt256> = self.masternodes.clone().into_keys().collect();
            let t2 = Instant::now();
            pro_tx_hashes
                .sort_by(|&h1, &h2|
                    h1.clone()
                        .reversed()
                        .cmp(&h2.clone()
                            .reversed()));
            let t3 = Instant::now();
            let mns = self.masternodes.clone();
            let t4 = Instant::now();

            let entry_hashes = pro_tx_hashes
                .clone()
                .into_iter()
                .map(|hash| {
                    let h = hash.clone();
                    let mn = &mns[&h];
                    let entry_hash = mn.masternode_entry_hash_at(block_height);
                    entry_hash
                })
                .collect();
            let t5 = Instant::now();
            println!("mndiff_process. hashes_for_merkle_root: clone nodes: {:?}, sort hashes: {:?}, clone again: {:?}, map hashes: {:?}, total: {:?}",
                     t2.duration_since(t1),
                     t3.duration_since(t2),
                     t4.duration_since(t3),
                     t5.duration_since(t4),
                     t5.duration_since(t0)
            );
            Some(entry_hashes)
        }
    }

    fn hashes_for_quorum_merkle_root(&self) -> Vec<UInt256> {
        let t0 = Instant::now();
        let mut llmq_commitment_hashes: Vec<UInt256> = self.quorums
            .clone()
            .into_values()
            .fold(Vec::new(), |mut acc, q_map| {
                let quorum_hashes: Vec<UInt256> = q_map
                    .into_values()
                    .map(|entry| entry.quorum_entry_hash)
                    .collect();
                acc.extend(quorum_hashes);
                acc
            });
        llmq_commitment_hashes.sort();
        println!("mndiff_process. hashes_for_quorum_merkle_root: {:?}", Instant::now().duration_since(t0));
        llmq_commitment_hashes
    }

    pub fn masternode_for(&self, registration_hash: UInt256) -> Option<&MasternodeEntry> {
        self.masternodes.get(&registration_hash)
    }
}
