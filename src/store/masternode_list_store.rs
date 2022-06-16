use std::cmp::{max, min};
use std::collections::{BTreeMap, BTreeSet};
use chrono::NaiveDateTime;
use dash_spv_models::common::{BlockData, ChainType, LLMQType};
use dash_spv_models::masternode::{LLMQEntry, MasternodeEntry, MasternodeList};
use dash_spv_primitives::crypto::byte_util::Zeroable;
use dash_spv_primitives::crypto::UInt256;
use dash_spv_storage::models::chain::merkle_block::MerkleBlock;
use dash_spv_storage::models::masternode::Masternode;
use dash_spv_storage::models::masternode::masternode::{delete_masternodes, delete_masternodes_with_empty_lists, save_plaform_ping_info};
use dash_spv_storage::models::masternode::masternode_list::{delete_masternode_list, delete_masternode_lists, masternode_list_for_block, masternode_lists_for_chain, masternode_lists_in};
use dash_spv_storage::models::masternode::quorum::{delete_quorums, delete_quorums_since_height};


pub const CHAINLOCK_ACTIVATION_HEIGHT: u32 = 1088640;

pub struct ChainContext<BL, BHL, BHASHL, GLTBH>
    where
        BL: Fn(i32) -> Option<BlockData> + Copy,
        BHL: Fn(u32) -> Option<BlockData> + Copy,
        BHASHL: Fn(UInt256) -> Option<BlockData> + Copy,
        GLTBH: Fn() -> Option<BlockData> + Copy,
{
    pub r#type: ChainType,
    pub chain_id: i32,
    pub block_lookup: BL,
    pub block_height_lookup: BHL,
    pub block_hash_lookup: BHASHL,
    pub get_last_terminal_block: GLTBH,
}

impl<BL, BHL, BHASHL, GLTBH> ChainContext<BL, BHL, BHASHL, GLTBH>
    where
        BL: Fn(i32) -> Option<BlockData> + Copy,
        BHL: Fn(u32) -> Option<BlockData> + Copy,
        BHASHL: Fn(UInt256) -> Option<BlockData> + Copy,
        GLTBH: Fn() -> Option<BlockData> + Copy {

    pub fn is_mainnet(&self) -> bool {
        self.r#type.is_mainnet()
    }

    pub fn get_estimated_block_height(&self) -> u32 {
        0
    }
    pub fn get_last_terminal_block_height(&self) -> u32 {
        0
    }

    pub fn get_block_at_height_or_last_terminal(&self, block_height: u32) -> Option<BlockData> {
        None
    }

    pub fn get_block_by_stored_id(&self, block_id: i32) -> Option<BlockData> {
        //let block = self.block_lookup(block_id);
        None
    }

    pub fn get_block_id_by_hash(&self, block_hash: UInt256) -> Option<i32> {
        None
    }

    pub fn get_height_for_block_hash(&self, block_hash: UInt256) -> u32 {
        //block_height_lookup(masternode_list.block_hash)

        0
    }
    pub fn get_quorum_type_for_platform(&self) -> LLMQType {
        match self.r#type {
            ChainType::MainNet => LLMQType::Llmqtype100_67,
            ChainType::TestNet => LLMQType::Llmqtype100_67,
            ChainType::DevNet => LLMQType::Llmqtype10_60
        }
    }

    pub fn get_quorum_type_for_chain_locks(&self) -> LLMQType {
        //DSLLMQType quorumType = self.chain.quorumTypeForChainLocks;
        match self.r#type {
            ChainType::MainNet => LLMQType::Llmqtype400_60,
            ChainType::TestNet => LLMQType::Llmqtype50_60,
            ChainType::DevNet => LLMQType::Llmqtype10_60
        }
    }

    pub fn get_quorum_type_for_is_locks(&self) -> LLMQType {
        //DSLLMQType quorumType = self.chain.quorumTypeForISLocks;
        match self.r#type {
            ChainType::MainNet => LLMQType::Llmqtype50_60,
            ChainType::TestNet => LLMQType::Llmqtype50_60,
            ChainType::DevNet => LLMQType::Llmqtype10_60
        }
    }

}

pub struct MasternodeListStore<'a, BL, BHL, BHASHL, GLTBH>
    where
        BL: Fn(i32) -> Option<BlockData> + Copy,
        BHL: Fn(u32) -> Option<BlockData> + Copy,
        BHASHL: Fn(UInt256) -> Option<BlockData> + Copy,
        GLTBH: Fn() -> Option<BlockData> + Copy {
    pub context: ChainContext<BL, BHL, BHASHL, GLTBH>,
    pub current_masternode_list: Option<MasternodeList>,
    pub masternode_list_awaiting_quorum_validation: Option<MasternodeList>,
    pub recent_masternode_lists: Vec<Masternode<'a>>,
    pub by_block_hash: BTreeMap<UInt256, MasternodeList>,
    pub block_hash_stubs: BTreeSet<UInt256>,
    pub queries_needing_quorums_validated: BTreeSet<UInt256>,
    pub cached_block_hash_heights: BTreeMap<UInt256, u32>,
    pub currently_being_saved_count: usize,
    // last by height, not by time queried
    pub last_queried_block_hash: Option<UInt256>,
    //@property (nonatomic, readwrite, nullable) NSData *processingMasternodeListDiffHashes;

}

impl<'a, BL, BHL, BHASHL, GLTBH> MasternodeListStore<'a, BL, BHL, BHASHL, GLTBH>
    where
        BL: Fn(i32) -> Option<BlockData> + Copy,
        BHL: Fn(u32) -> Option<BlockData> + Copy,
        BHASHL: Fn(UInt256) -> Option<BlockData> + Copy,
        GLTBH: Fn() -> Option<BlockData> + Copy {
    pub fn new(context: ChainContext<BL, BHL, BHASHL, GLTBH>) -> MasternodeListStore<'a, BL, BHL, BHASHL, GLTBH> {
        MasternodeListStore {
            context,
            current_masternode_list: None,
            masternode_list_awaiting_quorum_validation: None,
            recent_masternode_lists: vec![],
            by_block_hash: BTreeMap::new(),
            block_hash_stubs: BTreeSet::new(),
            queries_needing_quorums_validated: BTreeSet::new(),
            cached_block_hash_heights: BTreeMap::new(),
            currently_being_saved_count: 0,
            last_queried_block_hash: None
        }
    }

    // pub fn setup(&mut self) {
    //     self.delete_empty_masternode_lists(self.context.chain_id); // this is just for sanity purposes
    //     self.load_masternode_lists_with_block_height_lookup(None);
    //     self.remove_old_masternodes();
    //     self.load_local_masternodes();
    // }

    pub fn save_platform_ping_info_for_entries(&self, entries: Vec<MasternodeEntry>, platform_ping: i64, platform_ping_date: NaiveDateTime) {
        // [context performBlockAndWait:^{
        //     for (DSSimplifiedMasternodeEntry *entry in entries) {
        //         [entry savePlatformPingInfoInContext:context];
        //     }
        //     NSError *savingError = nil;
        //     [context save:&savingError];
        // }];

        entries.iter().for_each(|entry| {
            save_plaform_ping_info(self.context.chain_id, entry.provider_registration_transaction_hash, platform_ping, platform_ping_date)
                .expect("Error saving platform_ping info");
        });
    }



    pub fn height_for_block_hash(&mut self, block_hash: UInt256) -> u32 {
        if block_hash.is_zero() {
            return 0;
        }
        if let Some(cached) = self.cached_block_hash_heights.get(&block_hash) {
            return *cached;
        }
        let chain_height = self.context.get_height_for_block_hash(block_hash);
        if chain_height != u32::MAX {
            self.cached_block_hash_heights.insert(block_hash.clone(), chain_height);
        }
        chain_height
    }

    pub fn height_for_masternode_list(&self, masternode_list: &MasternodeList) -> u32 {
        if masternode_list.known_height == 0 || masternode_list.known_height == u32::MAX {
            self.context.get_height_for_block_hash(masternode_list.block_hash)
        } else {
            masternode_list.known_height
        }
    }


    pub fn earliest_masternode_list_block_height(&mut self) -> u32 {
        let mut earliest = u32::MAX;
        self.block_hash_stubs.clone().iter().for_each(|&hash| {
            earliest = min(earliest, self.height_for_block_hash(hash));
        });
        self.by_block_hash.clone().keys().for_each(|&hash| {
            earliest = min(earliest, self.height_for_block_hash(hash));
        });
        earliest
    }

    pub fn known_masternode_lists_count(&self) -> usize {
        let hashes: BTreeSet<UInt256> = self.by_block_hash.clone().into_keys().collect();
        self.block_hash_stubs.union(&hashes).count()
    }

    pub fn last_masternode_list_block_height(&mut self) -> u32 {
        let mut last = 0u32;
        self.block_hash_stubs.clone().iter().for_each(|&hash| {
            last = max(last, self.height_for_block_hash(hash));
        });
        self.by_block_hash.clone().keys().for_each(|&hash| {
            last = max(last, self.height_for_block_hash(hash));
        });
        if last > 0 { last } else { u32::MAX }
    }

    pub fn masternode_list_before_block_hash(&mut self, block_hash: UInt256) -> Option<MasternodeList> {
        let mut min_distance = u32::MAX;
        let mut closest_masternode_list = None;

        let block_height = self.height_for_block_hash(block_hash);
        self.by_block_hash.clone().iter().for_each(|(&key, value)| {
            let distance = block_height - self.height_for_block_hash(key);
            if distance > 0 && distance < min_distance {
                min_distance = distance;
                closest_masternode_list = Some(value.clone());
            }
        });
        if let Some(ref list) = closest_masternode_list {
            if self.context.is_mainnet() &&
                self.height_for_masternode_list(&list) < CHAINLOCK_ACTIVATION_HEIGHT &&
                block_height >= CHAINLOCK_ACTIVATION_HEIGHT {
                // special mainnet case
                return None;
            }
        }
        closest_masternode_list
    }

    pub fn masternode_list_for_block_hash(&mut self, block_hash: UInt256, block_height_lookup: Option<BHL>) -> Option<MasternodeList> {
        if let Some(list) = self.by_block_hash.get(&block_hash) {
            return Some(list.clone())
        }
        if self.block_hash_stubs.contains(&block_hash) {
            if let Some(list) = self.load_masternode_list_at_block_hash(block_hash, block_height_lookup) {
                return Some(list)
            }
        }
        println!("No masternode list at {} {}", block_hash, self.context.get_height_for_block_hash(block_hash));
        None
    }

    pub fn recent_masternode_lists(&self) -> Vec<MasternodeList> {
        let mut values: Vec<MasternodeList> = self.by_block_hash.clone().into_values().collect();
        values.sort_by(|l1, l2|
            self.height_for_masternode_list(l1)
                .cmp(&self.height_for_masternode_list(l2)));
        values
    }

    pub fn delete_all_on_chain(&self, chain_id: i32) {
        let _ = delete_masternodes(chain_id);
        let _ = delete_quorums(chain_id);
        let _ = delete_masternode_lists(chain_id);
    }

    pub fn delete_empty_masternode_lists(&self, chain_id: i32) {

        //let _ = delete_masternodes()
    }

    pub fn has_blocks_with_hash(&self, block_hash: UInt256) -> bool {
        // [DSMerkleBlockEntity
        // countObjectsInContext: self.managedObjectContext
        // matching: @ "blockHash == %@", uint256_data(blockHash)]
        false
    }

    pub fn has_masternode_list_at(&self, block_hash: UInt256) -> bool {
        self.by_block_hash.get(&block_hash).is_some() | self.block_hash_stubs.get(&block_hash).is_some()
    }

    pub fn has_masternode_list_currently_being_saved(&self, block_hash: UInt256) -> bool {
        self.currently_being_saved_count > 0
    }

    pub fn masternode_lists_to_sync(&mut self) -> u32 {
        let last = self.last_masternode_list_block_height();
        if last == u32::MAX {
            32
        } else {
            let diff = self.context.get_estimated_block_height() - last;
            if diff < 0 {
                32
            } else {
                min(32, ((diff / 24) as f64).ceil() as u32)
            }
        }
    }

    pub fn masternode_lists_and_quorums_is_synced(&mut self) -> bool {
        let last = self.last_masternode_list_block_height();
        if last == u32::MAX || last < self.context.get_estimated_block_height() - 16 {
            false
        } else {
            true
        }
    }


    pub fn load_local_masternodes(&self) {}

    pub fn load_masternode_list_at_block_hash(&mut self, block_hash: UInt256, block_height_lookup: Option<BHL>) -> Option<MasternodeList> {
        // TODO: callback to ffi DB or change to block_hash or use full DB here
        let block_id = 0;
        let bhl = block_height_lookup.unwrap();
        let BHL_temp = |hash: UInt256| bhl(0).unwrap().height;
        match masternode_list_for_block(self.context.chain_id, block_id) {
            Ok(entity_list) => {
                let list = entity_list.masternode_list_with_simplified_masternode_entry_pool_and_lookup(BTreeMap::new(), BTreeMap::new(), BHL_temp);
                self.by_block_hash.insert(block_hash, list.clone());
                self.block_hash_stubs.remove(&block_hash);
                println!("Loading Masternode List at height {} for blockHash {} with {} entries", self.height_for_masternode_list(&list), block_hash, list.masternodes.len());
                Some(list)
            },
            Err(err) => None
        }
    }

    pub fn load_masternode_lists_with_block_height_lookup(&mut self, block_height_lookup: Option<BHL>) {
        let BHL_temp = |hash: UInt256| block_height_lookup.unwrap()(0).unwrap().height;

        // TODO: need data indexed by block height (ASC)
        match masternode_lists_for_chain(self.context.chain_id) {
            Ok(entities) => {
                let mut needed_masternode_list_height = self.context.get_last_terminal_block_height() - 23; //2*8+7
                let count = entities.len();
                let mut simplified_masternode_entry_pool: BTreeMap<UInt256, MasternodeEntry> = BTreeMap::new();
                let mut quorum_entry_pool: BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>> = BTreeMap::new();
                entities.iter().rev().enumerate().for_each(|(i, entity)| {
                    // either last one or there are less than 3 (we aim for 3)
                    // TODO: need to fetch block_height and hash by entity.block_id
                    let entity_block_data = BlockData { height: 0, hash: Default::default() };
                    let entity_block_height: u32 = entity_block_data.height;
                    let entity_block_hash: UInt256 = entity_block_data.hash;
                    let is_last = i == count - 1;
                    if is_last || (self.by_block_hash.len() < 3 && needed_masternode_list_height >= entity_block_height) {
                        // we only need a few in memory as new quorums will mostly be verified against recent masternode lists
                        let masternode_list = entity.masternode_list_with_simplified_masternode_entry_pool_and_lookup(
                            simplified_masternode_entry_pool.clone(),
                            quorum_entry_pool.clone(),
                            BHL_temp);
                        self.cached_block_hash_heights.insert(masternode_list.block_hash, entity_block_height);
                        self.by_block_hash.insert(masternode_list.block_hash, masternode_list.clone());
                        simplified_masternode_entry_pool.extend(masternode_list.masternodes.clone());
                        quorum_entry_pool.extend(masternode_list.quorums.clone());
                        let height = self.height_for_masternode_list(&masternode_list);
                        println!("Loading Masternode List at height {} for blockHash {} with {} entries", height, masternode_list.block_hash, &masternode_list.masternodes.len());
                        if is_last {
                            self.masternode_list_awaiting_quorum_validation = Some(masternode_list.clone());
                        }
                        needed_masternode_list_height = entity_block_height - 8;
                    } else {
                        // just keep a stub around
                        self.cached_block_hash_heights.insert(entity_block_hash, entity_block_height);
                        self.block_hash_stubs.insert(entity_block_hash);
                    }
                });
            },
            Err(_err) => {}
        }
    }

    pub fn reload_masternode_lists_with_block_height_lookup(&mut self, block_height_lookup: Option<BHL>) {
        self.remove_all_masternode_lists();
        self.masternode_list_awaiting_quorum_validation = None;
        self.load_masternode_lists_with_block_height_lookup(block_height_lookup);
    }

    pub fn remove_all_masternode_lists(&mut self) {
        self.by_block_hash.clear();
        self.block_hash_stubs.clear();
        self.masternode_list_awaiting_quorum_validation = None;
    }

    pub fn remove_old_masternode_lists(&mut self) {
        match &self.current_masternode_list {
            Some(current_masternode_list) => {
                let chain_id = self.context.chain_id;
                let last_block_height = self.height_for_masternode_list(&current_masternode_list);
                let mut masternode_list_block_hashes = self.by_block_hash.keys().copied().collect::<BTreeSet<UInt256>>();
                masternode_list_block_hashes.extend(self.block_hash_stubs.clone());
                let block_ids = masternode_list_block_hashes.iter().filter_map(|&hash| self.context.get_block_id_by_hash(hash)).collect();
                if let Ok(entities) = masternode_lists_in(chain_id, block_ids) {
                    //[DSMasternodeListEntity objectsInContext:self.managedObjectContext matching:@"block.height < %@ && block.blockHash IN %@ && (block.usedByQuorums.@count == 0)", @(last_block_height - 50), masternode_list_block_hashes];
                    let removed_items = entities.len() > 0;

                    entities.iter().for_each(|entity| {
                        let block_id = entity.block_id;
                        let block = self.context.get_block_by_stored_id(block_id).unwrap();
                        //println!("Removing masternodeList at height: {} where quorums are: %@", block.height, block.usedByQuorums);
                        // A quorum is on a block that can only have one masternode list.
                        // A block can have one quorum of each type.
                        // A quorum references the masternode list by it's block
                        // we need to check if this masternode list is being referenced by a quorum using the inverse of quorum.block.masternodeList
                        let _ = delete_masternode_list(chain_id, block_id);
                        self.by_block_hash.remove(&block.hash);
                    });
                    if removed_items {
                        // Now we should delete old quorums
                        // To do this, first get the last 24 active masternode lists
                        // Then check for quorums not referenced by them, and delete those
                        // [DSMasternodeListEntity objectsSortedBy:@"block.height" ascending:NO offset:0 limit:10 inContext:self.managedObjectContext]
                        let recent_masternode_lists = masternode_lists_for_chain(chain_id);
                        let old_time = last_block_height - 24;

                        let oldest_block_height = match recent_masternode_lists {
                            Ok(lists) => if lists.len() > 0 {

                                min(self.context.get_block_by_stored_id(lists.iter().last().unwrap().block_id).unwrap().height, old_time)
                            } else {
                                old_time
                            },
                            Err(_err) => old_time
                        };
                        let _ = delete_quorums_since_height(self.context.chain_id, oldest_block_height);
                    }
                }
            }
            _ => {}
        }
    }

    /// this serves both for cleanup, but also for initial migration
    pub fn remove_old_masternodes(&mut self) {
        let _count = delete_masternodes_with_empty_lists(self.context.chain_id);
    }

    // pub fn masternode_entry_with_pro_reg_tx_hash(&self, pro_reg_tx_hash: UInt256) -> Option<MasternodeEntry> {
    //     self.current_masternode_list?.masternodes.get(&pro_reg_tx_hash)
    // }
    //
    // pub fn masternode_entry_for_location(&self, address: SocketAddress) -> Option<&MasternodeEntry> {
    //     self.current_masternode_list?.masternodes.values().filter(|&entry| entry.socket_address.eq(&address)).last()
    // }


    pub fn quorum_entry_for_platform_having_quorum_hash(&mut self, quorum_hash: UInt256, block_height: u32) -> Option<LLMQEntry> {
        if let Some(block) = self.context.get_block_at_height_or_last_terminal(block_height) {
            self.quorum_entry_for_platform_having_quorum_hash_by_block(quorum_hash, block)
        } else {
            None
        }
    }

    pub fn quorum_entry_for_platform_having_quorum_hash_by_block(&mut self, quorum_hash: UInt256, block: BlockData) -> Option<LLMQEntry> {
        match self.masternode_list_for_block_hash(block.hash, None)
            .or(self.masternode_list_before_block_hash(block.hash)) {
            Some(list) => {
                if block.height - self.height_for_masternode_list(&list) > 32 {
                    println!("Masternode list is too old");
                    return None;
                }
                if let Some(quorum) = list.quorum_entry_for_platform_with_quorum_hash(quorum_hash, self.context.get_quorum_type_for_platform()) {
                    Some(quorum.clone())
                } else {
                    self.quorum_entry_for_platform_having_quorum_hash(quorum_hash, block.height - 1)
                }
            },
            None => {
                println!("No masternode list found yet");
                return None;
            }
        }
    }

    pub fn quorum_entry_for_lock_request_id(&mut self, request_id: UInt256, llmq_type: LLMQType, block: MerkleBlock, expiration_offset: u32) -> Option<LLMQEntry> {
        match self.masternode_list_before_block_hash(block.block_hash) {
            Some(list) => {
                let height = self.height_for_masternode_list(&list);
                let block_height = block.height as u32;
                let delta = block_height - height;
                if delta > expiration_offset {
                    println!("Masternode list for is too old (age: {} masternodeList height {} merkle block height {})", delta, height, block_height);
                    None
                } else if let Some(quorum) = list.quorum_entry_for_lock_request_id(request_id, llmq_type) {
                    Some(quorum.clone())
                } else {
                    None
                }
            },
            None => {
                println!("No masternode list found yet");
                return None;
            }
        }
    }

    pub fn quorum_entry_for_chain_lock_request_id(&mut self, request_id: UInt256, block: MerkleBlock) -> Option<LLMQEntry> {
        self.quorum_entry_for_lock_request_id(request_id, self.context.get_quorum_type_for_chain_locks(), block, 24)
    }

    pub fn quorum_entry_for_instant_send_lock_request_id(&mut self, request_id: UInt256, block: MerkleBlock) -> Option<LLMQEntry> {
        self.quorum_entry_for_lock_request_id(request_id, self.context.get_quorum_type_for_is_locks(), block, 32)
    }



    pub fn add_block_to_validation_queue(&mut self, block: MerkleBlock) -> bool {
        let merkle_block_hash = block.block_hash;
        println!("add_block_to_validation_queue: {}:{}", block.height, merkle_block_hash);
        if self.has_masternode_list_at(merkle_block_hash) {
            println!("Already have that masternode list (or in stub) {}", block.height);
            return false;
        }
        self.last_queried_block_hash = Some(merkle_block_hash);
        self.queries_needing_quorums_validated.insert(merkle_block_hash);
        true
    }
    //
    //
    // pub fn prepare_to_save_masternode_list(&mut self, masternode_list: MasternodeList, added_masternodes: BTreeMap<UInt256, MasternodeEntry>, modified_masternodes: BTreeMap<UInt256, MasternodeEntry>, added_quorums: BTreeMap<LLMQType, BTreeMap<UInt256, LLMQEntry>>) {
    //     let block_hash = masternode_list.block_hash;
    //     if self.has_masternode_list_at(block_hash) {
    //         // in rare race conditions this might already exist
    //         return;
    //     }
    //     //let updated_simplified_masternode_entries: Vec<MasternodeEntry> = added_masternodes.values().collect() + modified_masternodes.values();
    //
    //     // [self.chain updateAddressUsageOfSimplifiedMasternodeEntries:updatedSimplifiedMasternodeEntries];
    //
    //     self.by_block_hash.insert(block_hash, masternode_list);
    //
    //     // dispatch_async(dispatch_get_main_queue(), ^{
    //     //     [[NSNotificationCenter defaultCenter] postNotificationName:DSMasternodeListDidChangeNotification object:nil userInfo:@{DSChainManagerNotificationChainKey: self.chain}];
    //     //     [[NSNotificationCenter defaultCenter] postNotificationName:DSQuorumListDidChangeNotification object:nil userInfo:@{DSChainManagerNotificationChainKey: self.chain}];
    //     // });
    //
    //     // We will want to create unknown blocks if they came from insight
    //     let create_unknown_blocks = !self.context.is_mainnet();
    //     self.currently_being_saved_count += 1;
    //     // This will create a queue for masternodes to be saved without blocking the networking queue
    //
    //
    //
    //     // dispatch_async(self.masternodeSavingQueue, ^{
    //     //     [DSMasternodeListStore saveMasternodeList:masternodeList
    //     //     toChain:self.chain
    //     //     havingModifiedMasternodes:modifiedMasternodes
    //     //     addedQuorums:addedQuorums
    //     //     create_unknown_blocks:createUnknownBlocks
    //     //     inContext:self.managedObjectContext
    //     //     completion:^(NSError *error) {
    //     //         self.masternodeListCurrentlyBeingSavedCount--;
    //     //         completion(error);
    //     //     }];
    //     // });
    // }
    //
    // //+ (void)saveMasternodeList:(DSMasternodeList *)masternodeList toChain:(DSChain *)chain havingModifiedMasternodes:(NSDictionary *)modifiedMasternodes addedQuorums:(NSDictionary *)addedQuorums createUnknownBlocks:(BOOL)createUnknownBlocks inContext:(NSManagedObjectContext *)context completion:(void (^)(NSError *error))completion {
    //
    // pub fn save_masternode_list(masternode_list: MasternodeList) {
    //     // let mnl_height = masternode_list.height;
    //     // println!("Queued saving MNL at height {}", mnl_height);
    //     let mnl_block_hash = masternode_list.block_hash;
    //     //masternodes
    //     //DSChainEntity *chainEntity = [chain chainEntityInContext:context];
    //
    //     let merkle_block_entity = MerkleBlock::merkle_block_with_hash(mnl_block_hash);
    //
    // }

}
