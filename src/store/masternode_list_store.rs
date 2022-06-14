use std::cmp::{max, min};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use dash_spv_models::common::{BlockData, ChainType, LLMQType, SocketAddress};
use dash_spv_models::common::ChainType::MainNet;
use dash_spv_models::masternode::{LLMQEntry, MasternodeEntry, MasternodeList};
use dash_spv_primitives::crypto::byte_util::{Reversable, Zeroable};
use dash_spv_primitives::crypto::{UInt128, UInt256};
use dash_spv_storage::models::chain::merkle_block::MerkleBlock;
use dash_spv_storage::models::masternode::Masternode;
use dash_spv_storage::models::masternode::masternode::{delete_masternodes, delete_masternodes_with_empty_lists};
use dash_spv_storage::models::masternode::masternode_list::{delete_masternode_list, delete_masternode_lists, masternode_list_for_block, masternode_lists_for_chain, masternode_lists_in};
use dash_spv_storage::models::masternode::quorum::{delete_quorums, delete_quorums_since_height, quorums_since_height};


pub const CHAINLOCK_ACTIVATION_HEIGHT: u32 = 1088640;

pub struct ChainContext<BL>
    where
        BL: Fn(i32) -> Option<BlockData>,
        BHL: Fn(u32) -> Option<BlockData>,
        GLTBH: Fn() -> Option<BlockData>,
{
    pub r#type: ChainType,
    pub chain_id: i32,
    pub block_lookup: BL,
    pub block_height_lookup: BL,
    pub get_last_terminal_block: GLTBH,
}

pub struct MasternodeListStore<'a, BL> where BL: Fn(i32) -> BlockData + Copy {
    pub context: ChainContext<BL>,
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

impl<'a, BL> MasternodeListStore<'a, BL> where BL: Fn(i32) -> BlockData + Copy {

    pub fn new(context: ChainContext<BL>) -> MasternodeListStore<'a, BL> {
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

    pub fn setup(&mut self) {
        self.delete_empty_masternode_lists(self.context.chain_id); // this is just for sanity purposes
        self.load_masternode_lists_with_block_height_lookup(None);
        self.remove_old_masternodes();
        self.load_local_masternodes();
    }

    pub fn height_for_block_hash(&mut self, block_hash: UInt256) -> u32 {
        if block_hash.is_zero() {
            return 0;
        }
        if let Some(cached) = self.cached_block_hash_heights.get(&block_hash) {
            return *cached;
        }
        // let chain_height = [self.chain heightForBlockHash:blockhash];
        let chain_height = u32::MAX;
        if chain_height != u32::MAX {
            self.cached_block_hash_heights.insert(block_hash, chain_height);
        }
        chain_height
    }

    pub fn earliest_masternode_list_block_height(&mut self) -> u32 {
        let mut earliest = u32::MAX;
        self.block_hash_stubs.iter().for_each(|hash| {
            earliest = min(earliest, self.height_for_block_hash(*hash));
        });
        self.by_block_hash.iter().for_each(|(hash)| {
            earliest = min(earliest, self.height_for_block_hash(*hash));
        });
        earliest
    }

    pub fn known_masternode_lists_count(&self) -> usize {
        let mut known = self.block_hash_stubs.clone();
        known.extend(self.by_block_hash.into_keys());
        known.len()
    }

    pub fn last_masternode_list_block_height(&mut self) -> u32 {
        let mut last = 0u32;
        self.block_hash_stubs.iter().for_each(|hash| {
            last = max(last, self.height_for_block_hash(*hash));
        });
        self.by_block_hash.iter().for_each(|(hash)| {
            last = max(last, self.height_for_block_hash(*hash));
        });
        if last > 0 { last } else { u32::MAX }
    }

    pub fn recent_masternode_lists(&self) -> Vec<MasternodeList> {
        let values = self.by_block_hash.values().collect::<Vec<MasternodeList>>();
        // values.sort_by()
       // return [[self.masternodeListsByBlockHash allValues]
        // sortedArrayUsingDescriptors:@[[NSSortDescriptor sortDescriptorWithKey:@"height" ascending:YES]]];

        //values.sort_by()

        // self.by_block_hash

        //self.by_block_hash
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
            let diff = self.chain.estimatedBlockHeight - last;
            if diff  < 0 {
                32
            } else {
                min(32, unsafe { ceilf32(diff / 24) } as u32)
            }
        }
    }

    pub fn masternode_lists_and_quorums_is_synced(&mut self) -> bool {
        let last = self.last_masternode_list_block_height();
        if last == u32::MAX || last  < self.chain.estimatedBlockHeight - 16 {
            false
        } else {
            true
        }
    }

    pub fn load_local_masternodes(&self) {

    }

    pub fn load_masternode_list_at_block_hash<BHL>(&mut self, block_hash: UInt256, block_height_lookup: BHL) -> Option<MasternodeList>
        where BHL: Fn(UInt256) -> u32 + Copy {
        /// TODO: callback to ffi DB or change to block_hash or use full DB here
        let block_id = 0;
        match masternode_list_for_block(self.context.chain_id, block_id) {
            Ok(entity_list) => {
                let list = entity_list.masternode_list_with_simplified_masternode_entry_pool_and_lookup(BTreeMap::new(), HashMap::new(), block_height_lookup);
                self.by_block_hash.insert(block_hash, list.clone());
                self.block_hash_stubs.remove(&block_hash);
                let height = if list.known_height == 0 || list.known_height == u32::MAX {
                    block_height_lookup(list.block_hash)
                } else {
                    list.known_height
                };
                println!("Loading Masternode List at height {} for blockHash {} with {} entries", height, block_hash, list.masternodes.len());

                Some(list)
            },
            Err(err) => None
        }
    }

    pub fn load_masternode_lists_with_block_height_lookup<BHL>(&mut self, block_height_lookup: BHL)
        where BHL: Fn(UInt256) -> u32 + Copy {

        // TODO: need data indexed by block height (ASC)
        match masternode_lists_for_chain(self.context.chain_id) {
            Ok(entities) => {
                let needed_masternode_list_height = self.chain.lastTerminalBlockHeight - 23; //2*8+7
                let count = entities.len();
                let mut simplified_masternode_entry_pool: BTreeMap<UInt256, MasternodeEntry> = BTreeMap::new();
                let mut quorum_entry_pool: HashMap<LLMQType, HashMap<UInt256, LLMQEntry>> = HashMap::new();
                entities.iter().rev().enumerate().for_each(|(i, &entity)| {
                    // either last one or there are less than 3 (we aim for 3)
                    // TODO: need to fetch block_height and hash by entity.block_id
                    let entity_block_data = BlockData { height: 0, hash: Default::default() };
                    let entity_block_height: u32 = entity_block_data.height;
                    let entity_block_hash: UInt256 = entity_block_data.hash;
                    let is_last = i == count - 1;
                    if is_last || (self.by_block_hash.len() < 3 && needed_masternode_list_height >= entity_block_height) {
                        // we only need a few in memory as new quorums will mostly be verified against recent masternode lists
                        let masternode_list = entity.masternode_list_with_simplified_masternode_entry_pool_and_lookup(simplified_masternode_entry_pool, quorum_entry_pool, block_height_lookup);
                        self.cached_block_hash_heights.insert(masternode_list.block_hash, entity_block_height);
                        self.by_block_hash.insert(masternode_list.block_hash, masternode_list.clone());
                        simplified_masternode_entry_pool.extend(masternode_list.masternodes);
                        quorum_entry_pool.extend(masternode_list.quorums);
                        let height = if masternode_list.known_height == 0 || masternode_list.known_height == u32::MAX {
                            block_height_lookup(masternode_list.block_hash)
                        } else {
                            masternode_list.known_height
                        };
                        println!("Loading Masternode List at height {} for blockHash {} with {} entries", height, masternode_list.block_hash, masternode_list.masternodes.len());
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

    pub fn reload_masternode_lists_with_block_height_lookup<BHL>(&mut self, block_height_lookup: BHL)
        where BHL: Fn(UInt256) -> u32 + Copy {
        self.remove_all_masternode_lists();
        self.masternode_list_awaiting_quorum_validation = None;
        self.load_masternode_lists_with_block_height_lookup(block_height_lookup);
    }

    pub fn masternode_list_before_block_hash(&mut self, block_hash: UInt256) -> Option<MasternodeList> {
        let mut min_distance = u32::MAX;
        let block_height = self.height_for_block_hash(block_hash);
        let mut closest_masternode_list = None;
        self.by_block_hash.iter().for_each(|(&key, &value)| {
            let masternode_list_block_height = self.height_for_block_hash(key);

            if block_height > masternode_list_block_height {

                let distance = block_height - masternode_list_block_height;
                if distance < min_distance {
                    min_distance = distance;
                    closest_masternode_list = self.by_block_hash.get(&key);
                }
            }
        });
        if let Some(list) = closest_masternode_list {
            let height = if list.known_height == 0 || list.known_height == u32::MAX {
                block_height_lookup(list.block_hash)
            } else {
                list.known_height
            };
            if self.context.r#type == ChainType::MainNet && height < CHAINLOCK_ACTIVATION_HEIGHT && block_height >= CHAINLOCK_ACTIVATION_HEIGHT {
                // special mainnet case
                return None;
            }

        }

        closest_masternode_list
    }

    pub fn masternode_list_for_block_hash<BHL>(&mut self, mut block_hash: UInt256, block_height_lookup: BHL) -> Option<MasternodeList>
        where BHL: Fn(UInt256) -> u32 + Copy {
        if let Some(list) = self.by_block_hash.get(&block_hash) {
            return Some(list.clone())
        }
        if self.block_hash_stubs.contains(&block_hash) {
            if let Some(list) = self.load_masternode_list_at_block_hash(block_hash, block_height_lookup) {
                return Some(list.clone())
            }
        }
        println!("No masternode list at {} {}", block_hash, block_height_lookup(block_hash));
        None
    }

    pub fn remove_all_masternode_lists(&mut self) {
        self.by_block_hash.clear();
        self.block_hash_stubs.clear();
        self.masternode_list_awaiting_quorum_validation = None;
        self.masternode_list_awaiting_quorum_validation = None;
    }

    pub fn remove_old_masternode_lists(&mut self) {
        match self.current_masternode_list {
            Some(current_masternode_list) => {
                let chain_id = self.context.chain_id;
                let last_block_height = current_masternode_list.height;
                let mut masternode_list_block_hashes = self.by_block_hash.keys().copied().collect::<BTreeSet<UInt256>>();
                masternode_list_block_hashes.extend(self.block_hash_stubs.clone());
                if let Ok(entities) = masternode_lists_in(chain_id, masternode_list_block_hashes) {
                    //[DSMasternodeListEntity objectsInContext:self.managedObjectContext matching:@"block.height < %@ && block.blockHash IN %@ && (block.usedByQuorums.@count == 0)", @(last_block_height - 50), masternode_list_block_hashes];
                    let removed_items = entities.len() > 0;

                    entities.iter().for_each(|&entity| {
                        let block_id = entity.block_id;
                        let block = self.context.block_lookup(block_id);
                        println!("Removing masternodeList at height: {} where quorums are: %@", block.height, block.usedByQuorums);
                        // A quorum is on a block that can only have one masternode list.
                        // A block can have one quorum of each type.
                        // A quorum references the masternode list by it's block
                        // we need to check if this masternode list is being referenced by a quorum using the inverse of quorum.block.masternodeList
                        let _ = delete_masternode_list(chain_id, block_id);
                        self.by_block_hash.remove(block.hash);
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
                                min(self.context.block_lookup(lists.iter().last().unwrap().block_id).height, old_time)
                            } else {
                                old_time
                            },
                            Err(_err) => old_time
                        };
                        _ = delete_quorums_since_height(self.context.chain_id, oldest_block_height);

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

    pub fn masternode_entry_with_pro_reg_tx_hash(&self, pro_reg_tx_hash: UInt256) -> Option<&MasternodeEntry> {
        self.current_masternode_list?.masternodes.get(&pro_reg_tx_hash)
    }

    pub fn masternode_entry_for_location(&self, address: SocketAddress) -> Option<&MasternodeEntry> {
        self.current_masternode_list?.masternodes.values().filter(|&entry| entry.socket_address.eq(&address)).last()
    }
    pub fn quorumEntryForPlatformHavingQuorumHashByBlock(&self, quorum_hash: UInt256, block: BlockData) -> Option<LLMQEntry> {

    }

    pub fn quorumEntryForPlatformHavingQuorumHashByHeight(&self, quorum_hash: UInt256, block_height: u32) -> Option<LLMQEntry> {
        if let Some(block) = self.context.block_height_lookup(block_height) {
            self.quorumEntryForPlatformHavingQuorumHash(quorum_hash, block)
        } else if let Some(block) = self.context.get_last_terminal_block() {
            if block_height > block.height {
                self.quorumEntryForPlatformHavingQuorumHash(quorum_hash, block)

            } else {
                return nil;
            }

        }
    }

    pub fn quorum_entry_for_chain_lock_requestid(&mut self, request_id: UInt256, merkle_block: MerkleBlock) -> Option<LLMQEntry> {
        let masternode_list = self.masternode_list_before_block_hash(merkle_block.block_hash);
    }


    - (DSQuorumEntry *_Nullable)quorumEntryForPlatformHavingQuorumHash:(UInt256)quorumHash forBlockHeight:(uint32_t)blockHeight {
    DSBlock *block = [self.chain blockAtHeight:blockHeight];
    if (block == nil) {
    if (blockHeight > self.chain.lastTerminalBlockHeight) {
    block = self.chain.lastTerminalBlock;
    } else {
    return nil;
    }
    }
    return [self quorumEntryForPlatformHavingQuorumHash:quorumHash forBlock:block];
}

    pub fn quorum_entry_for_chain_lock_requestid(&self, request_id: UInt256, merkle_block: MerkleBlock) -> Option<LLMQEntry> {
        if let Some(masternode_list) = self.masternode_list_before_block_hash(merkle_block.block_hash) {

        }
    }

- (DSQuorumEntry *)quorumEntryForChainLockRequestID:(UInt256)requestID forMerkleBlock:(DSMerkleBlock *)merkleBlock {
DSMasternodeList *masternodeList = [self masternodeListBeforeBlockHash:merkleBlock.blockHash];
if (!masternodeList) {
DSLog(@"No masternode list found yet");
return nil;
}
if (merkleBlock.height - masternodeList.height > 24) {
DSLog(@"Masternode list is too old");
return nil;
}
return [masternodeList quorumEntryForChainLockRequestID:requestID];
}


    pub fn prepare_to_save_masternode_list(&mut self, masternode_list: MasternodeList, added_masternodes: BTreeMap<UInt256, MasternodeEntry>, modified_masternodes: BTreeMap<UInt256, MasternodeEntry>, added_quorums: HashMap<LLMQType, HashMap<UInt256, LLMQEntry>>) {
        let block_hash = masternode_list.block_hash;
        if self.has_masternode_list_at(block_hash) {
            // in rare race conditions this might already exist
            return;
        }
        let updated_simplified_masternode_entries: Vec<MasternodeEntry> = added_masternodes.values().collect() + modified_masternodes.values();

        // [self.chain updateAddressUsageOfSimplifiedMasternodeEntries:updatedSimplifiedMasternodeEntries];

        self.by_block_hash.insert(block_hash, masternode_list);

        // dispatch_async(dispatch_get_main_queue(), ^{
        //     [[NSNotificationCenter defaultCenter] postNotificationName:DSMasternodeListDidChangeNotification object:nil userInfo:@{DSChainManagerNotificationChainKey: self.chain}];
        //     [[NSNotificationCenter defaultCenter] postNotificationName:DSQuorumListDidChangeNotification object:nil userInfo:@{DSChainManagerNotificationChainKey: self.chain}];
        // });

        // We will want to create unknown blocks if they came from insight
        let create_unknown_blocks = self.context.r#type != MainNet;
        self.currently_being_saved_count += 1;
        // This will create a queue for masternodes to be saved without blocking the networking queue



        // dispatch_async(self.masternodeSavingQueue, ^{
        //     [DSMasternodeListStore saveMasternodeList:masternodeList
        //     toChain:self.chain
        //     havingModifiedMasternodes:modifiedMasternodes
        //     addedQuorums:addedQuorums
        //     create_unknown_blocks:createUnknownBlocks
        //     inContext:self.managedObjectContext
        //     completion:^(NSError *error) {
        //         self.masternodeListCurrentlyBeingSavedCount--;
        //         completion(error);
        //     }];
        // });
    }

    //+ (void)saveMasternodeList:(DSMasternodeList *)masternodeList toChain:(DSChain *)chain havingModifiedMasternodes:(NSDictionary *)modifiedMasternodes addedQuorums:(NSDictionary *)addedQuorums createUnknownBlocks:(BOOL)createUnknownBlocks inContext:(NSManagedObjectContext *)context completion:(void (^)(NSError *error))completion {

    pub fn save_masternode_list(masternode_list: MasternodeList) {
        let mnl_height = masternode_list.height;
        println!("Queued saving MNL at height %u", mnl_height);
        let mnl_block_hash = masternode_list.block_hash;
        //masternodes
        //DSChainEntity *chainEntity = [chain chainEntityInContext:context];

        let merkle_block_entity = MerkleBlock::merkle_block_with_hash(mnl_block_hash);

        if merkle_block_entity.is_err() {


        }


        if (!merkle_block_entity && ([chain checkpointForBlockHash:mnlBlockHash])) {
        DSCheckpoint *checkpoint = [chain checkpointForBlockHash:mnlBlockHash];
        DSBlock *block = [checkpoint blockForChain:chain];
        merkleBlockEntity = [[DSMerkleBlockEntity managedObjectInBlockedContext:context] setAttributesFromBlock:block forChainEntity:chainEntity];
        }

    }

}
