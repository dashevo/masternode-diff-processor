use std::cmp::min;
use std::collections::{BTreeMap, HashMap};
use crate::common::llmq_type::LLMQType;
use crate::consensus::Encodable;
use crate::crypto::byte_util::{merkle_root_from_hashes, Reversable, UInt256};
use crate::hashes::{Hash, sha256};
use crate::{log_masternodes_map, Zeroable};
use crate::masternode::quorum_entry::QuorumEntry;
use crate::masternode::masternode_entry::MasternodeEntry;

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
            // .field("quorums", &self.quorums)
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
            list.masternode_merkle_root = merkle_root_from_hashes(hashes);
        }
        if quorums_active {
            let hashes = list.hashes_for_quorum_merkle_root();
            list.quorum_merkle_root = merkle_root_from_hashes(hashes);
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
        let f = format!("validMasternodes.masternodes for {}: {}: {}", quorum_modifier, quorum_count, block_height);
        log_masternodes_map(self.masternodes.clone(), f);
        println!("scoreDictionaryForQuorumModifier: {}, {} [", quorum_modifier, block_height);
        let mut score_dictionary: BTreeMap<UInt256, MasternodeEntry> = self.masternodes.clone().into_iter().filter_map(|(h, entry)| {
            // let score = MasternodeList::masternode_score(entry.clone(), quorum_modifier, block_height);
            // println!("{}:{:?}", h, score);
            // if score.is_some() && !score.unwrap().0.is_empty() {
            //     Some((score.unwrap(), entry))
            // } else {
            //     None
            // }
            match MasternodeList::masternode_score(entry.clone(), quorum_modifier, block_height) {
                Some(score) => if score.is_zero() { None } else { Some((score, entry)) },
                None => None
            }

        }).collect();
        println!("]");
        let mut scores: Vec<UInt256> = score_dictionary.clone().into_keys().collect();
        scores.sort_by(|&s1, &s2| s2.clone().reversed().cmp(&s1.clone().reversed()));
        println!("validMasternodes.scores for {}: {}: {} [", quorum_modifier, quorum_count, block_height);
        for score in scores.clone() {
            println!("{:?}", score);
        }
        println!("]");
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

        println!("validMasternodes.nodes for {}: {}: {} [", quorum_modifier, quorum_count, block_height);
        for masternode in masternodes.clone() {
            println!("{:?}", masternode);
        }
        println!("]");
        masternodes
    }
//     - (UInt256)masternodeScore:(DSSimplifiedMasternodeEntry *)simplifiedMasternodeEntry modifier:(UInt256)modifier atBlockHeight:(uint32_t)blockHeight {
//     NSParameterAssert(simplifiedMasternodeEntry);
//
//     if (uint256_is_zero([simplifiedMasternodeEntry confirmedHashAtBlockHeight:blockHeight])) {
//     return UINT256_ZERO;
//     }
//     NSMutableData *data = [NSMutableData data];
//     [data appendData:[NSData dataWithUInt256:[simplifiedMasternodeEntry confirmedHashHashedWithProviderRegistrationTransactionHashAtBlockHeight:blockHeight]]];
//     [data appendData:[NSData dataWithUInt256:modifier]];
//     return data.SHA256;
// }

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
        if block_height == u32::MAX {
            println!("Block height lookup queried an unknown block {:?}", self.block_hash);
            None
        } else {
            let mut pro_tx_hashes: Vec<UInt256> = self.masternodes.clone().into_keys().collect();
            pro_tx_hashes
                .sort_by(|&h1, &h2|
                    h1.clone()
                        .reversed()
                        .cmp(&h2.clone()
                            .reversed()));
            let mns = self.masternodes.clone();
            let entry_hashes = pro_tx_hashes
                .clone()
                .into_iter()
                .map(|hash| {
                    let h = hash.clone();
                    let map = mns.clone();
                    let mn = &map[&h];
                    let entry_hash = mn.masternode_entry_hash_at(block_height);
                    entry_hash
                })
                .collect();
            Some(entry_hashes)
        }
    }

    fn hashes_for_quorum_merkle_root(&self) -> Vec<UInt256> {
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
        // println!("quorumMerkleRoot hashes: [");
        // for llmq_commitment_hash in llmq_commitment_hashes.clone() {
        //     println!("{}", llmq_commitment_hash);
        // }
        // println!("]");
        llmq_commitment_hashes
    }

    pub fn masternode_for(&self, registration_hash: UInt256) -> Option<&MasternodeEntry> {
        self.masternodes.get(&registration_hash)
    }
}
