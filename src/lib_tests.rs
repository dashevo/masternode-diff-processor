#[cfg(test)]
pub mod tests {

    use std::{env, fs, slice};
    use std::collections::HashMap;
    use std::io::Read;
    use std::ptr::null_mut;
    use byte::BytesExt;
    use dash_spv_ffi::ffi::from::FromFFI;
    use dash_spv_ffi::ffi::to::ToFFI;
    use dash_spv_ffi::ffi::unboxer::unbox_any;
    use dash_spv_ffi::types;
    use dash_spv_models::common::chain_type::ChainType;
    use dash_spv_models::masternode;
    use dash_spv_primitives::crypto::byte_util::{BytesDecodable, Reversable, UInt256, UInt384, UInt768};
    use dash_spv_primitives::hashes::hex::FromHex;
    use crate::LLMQType;
    use crate::mnl_diff_process;

    #[derive(Debug)]
    pub struct FFIContext {
        pub chain: ChainType,
    }

    pub struct AggregationInfo {
        pub public_key: UInt384,
        pub digest: UInt256,
    }

    pub fn block_height_for(chain: ChainType, key: &str) -> u32 {
        match chain {
            ChainType::MainNet => match key {
                "000000000000000bf16cfee1f69cd472ac1d0285d74d025caa27cebb0fb6842f" => 1090392,
                "000000000000000d6f921ffd1b48815407c1d54edc93079b7ec37a14a9c528f7" => 1090776,
                "000000000000000c559941d24c167053c5c00aea59b8521f5cef764271dbd3c5" => 1091280,
                "0000000000000003269a36d2ce1eee7753a2d2db392fff364f64f5a409805ca3" => 1092840,
                "000000000000001a505b133ea44b594b194f12fa08650eb66efb579b1600ed1e" => 1090368,
                "0000000000000006998d05eff0f4e9b6a7bab1447534eccb330972a7ef89ef65" => 1091424,
                "000000000000001d9b6925a0bc2b744dfe38ff7da2ca0256aa555bb688e21824" => 1090920,
                "000000000000000c22e2f5ca2113269ec62193e93158558c8932ba1720cea64f" => 1092648,
                "0000000000000020019489504beba1d6197857e63c44da3eb9e3b20a24f40d1e" => 1092168,
                "00000000000000112e41e4b3afda8b233b8cc07c532d2eac5de097b68358c43e" => 1088640,
                "00000000000000143df6e8e78a3e79f4deed38a27a05766ad38e3152f8237852" => 1090944,
                "0000000000000028d39e78ee49a950b66215545163b53331115e6e64d4d80328" => 1091184,
                "00000000000000093b22f6342de731811a5b3fa51f070b7aac6d58390d8bfe8c" => 1091664,
                "00000000000000037187889dd360aafc49d62a7e76f4ab6cd2813fdf610a7292" => 1092504,
                "000000000000000aee08f8aaf8a5232cc692ef5fcc016786af72bd9b001ae43b" => 1090992,
                "000000000000002395b6c4e4cb829556d42c659b585ee4c131a683b9f7e37706" => 1092192,
                "00000000000000048a9b52e6f46f74d92eb9740e27c1d66e9f2eb63293e18677" => 1091976,
                "000000000000001b4d519e0a9215e84c3007597cef6823c8f1c637d7a46778f0" => 1091448,
                "000000000000001730249b150b8fcdb1078cd0dbbfa04fb9a18d26bf7a3e80f2" => 1092528,
                "000000000000001c3073ff2ee0af660c66762af38e2c5782597e32ed690f0f72" => 1092072,
                "000000000000000c49954d58132fb8a1c90e4e690995396be91d8f27a07de349" => 1092624,
                "00000000000000016200a3f98e44f4b9e65da04b86bad799e6bbfa8972f0cead" => 1090080,
                "000000000000000a80933f2b9b8041fdfc6e94b77ba8786e159669f959431ff2" => 1092600,
                "00000000000000153afcdccc3186ad2ca4ed10a79bfb01a2c0056c23fe039d86" => 1092456,
                "00000000000000103bad71d3178a6c9a2f618d9d09419b38e9caee0fddbf664a" => 1092864,
                "000000000000001b732bc6d52faa8fae97d76753c8e071767a37ba509fe5c24a" => 1092360,
                "000000000000001a17f82d76a0d5aa2b4f90a6e487df366d437c34e8453f519c" => 1091112,
                "000000000000000caa00c2c24a385513a1687367157379a57b549007e18869d8" => 1090680,
                "0000000000000022e463fe13bc19a1fe654c817cb3b8e207cdb4ff73fe0bcd2c" => 1091736,
                "000000000000001b33b86b6a167d37e3fcc6ba53e02df3cb06e3f272bb89dd7d" => 1092744,
                "0000000000000006051479afbbb159d722bb8feb10f76b8900370ceef552fc49" => 1092432,
                "0000000000000008cc37827fd700ec82ee8b54bdd37d4db4319496977f475cf8" => 1091328,
                "0000000000000006242af03ba5e407c4e8412ef9976da4e7f0fa2cbe9889bcd2" => 1089216,
                "000000000000001dc4a842ede88a3cc975e2ade4338513d546c52452ab429ba0" => 1091496,
                "0000000000000010d30c51e8ce1730aae836b00cd43f3e70a1a37d40b47580fd" => 1092816,
                "00000000000000212441a8ef2495d21b0b7c09e13339dbc34d98c478cc51f8e2" => 1092096,
                "00000000000000039d7eb80e1bbf6f7f0c43f7f251f30629d858bbcf6a18ab58" => 1090728,
                "0000000000000004532e9c4a1def38cd71f3297c684bfdb2043c2aec173399e0" => 1091904,
                "000000000000000b73060901c41d098b91f69fc4f27aef9d7ed7f2296953e407" => 1090560,
                "0000000000000016659fb35017e1f6560ba7036a3433bfb924d85e3fdfdd3b3d" => 1091256,
                "000000000000000a3c6796d85c8c49b961363ee88f14bff10c374cd8dd89a9f6" => 1092696,
                "000000000000000f33533ba1c5d72f678ecd87abe7e974debda238c53b391737" => 1092720,
                "000000000000000150907537f4408ff4a8610ba8ce2395faa7e44541ce2b6c37" => 1090608,
                "000000000000001977d3a578e0ac3e4969675a74afe7715b8ffd9f29fbbe7c36" => 1091400,
                "0000000000000004493e40518e7d3aff585e84564bcd80927f96a07ec80259cb" => 1092480,
                "000000000000000df5e2e0eb7eaa36fcef28967f7f12e539f74661e03b13bdba" => 1090704,
                "00000000000000172f1765f4ed1e89ba4b717a475e9e37124626b02d566d31a2" => 1090632,
                "0000000000000018e62a4938de3428ddaa26e381139489ce1a618ed06d432a38" => 1092024,
                "000000000000000790bd24e65daaddbaeafdb4383c95d64c0d055e98625746bc" => 1091832,
                "0000000000000005f28a2cb959b316cd4b43bd29819ea07c27ec96a7d5e18ab7" => 1092408,
                "00000000000000165a4ace8de9e7a4ba0cddced3434c7badc863ff9e237f0c8a" => 1091088,
                "00000000000000230ec901e4d372a93c712d972727786a229e98d12694be9d34" => 1090416,
                "000000000000000bf51de942eb8610caaa55a7f5a0e5ca806c3b631948c5cdcc" => 1092336,
                "000000000000002323d7ba466a9b671d335c3b2bf630d08f078e4adee735e13a" => 1090464,
                "0000000000000019db2ad91ab0f67d90df222ce4057f343e176f8786865bcda9" => 1091568,
                "0000000000000004a38d87062bf37ef978d1fc8718f03d9222c8aa7aa8a4470f" => 1090896,
                "0000000000000022c909de83351791e0b69d4b4be34b25c8d54c8be3e8708c87" => 1091592,
                "0000000000000008f3dffcf342279c8b50e49c47e191d3df453fdcd816aced46" => 1092792,
                "000000000000001d1d7f1b88d6518e6248616c50e4c0abaee6116a72bc998679" => 1092048,
                "0000000000000020de87be47c5c10a50c9edfd669a586f47f44fa22ae0b2610a" => 1090344,
                "0000000000000014d1d8d12dd5ff570b06e76e0bbf55d762a94d13b1fe66a922" => 1091760,
                "000000000000000962d0d319a96d972215f303c588bf50449904f9a1a8cbc7c2" => 1089792,
                "00000000000000171c58d1d0dbae71973530aa533e4cd9cb2d2597ec30d9b129" => 1091352,
                "0000000000000004acf649896a7b22783810d5913b31922e3ea224dd4530b717" => 1092144,
                "0000000000000013479b902955f8ba2d4ce2eb47a7f9f8f1fe477ec4b405bddd" => 1090512,
                "000000000000001be0bbdb6b326c98ac8a3e181a2a641379c7d4308242bee90b" => 1092216,
                "000000000000001c09a68353536ccb24b51b74c642d5b6e7e385cff2debc4e64" => 1092120,
                "0000000000000013974ed8e13d0a50f298be0f2b685bfcfd8896172db6d4a145" => 1090824,
                "000000000000001dbcd3a23c131fedde3acd6da89275e7f9fcae03f3107da861" => 1092888,
                "000000000000000a8812d75979aac7c08ac69179037409fd7a368372edd05d23" => 1090872,
                "000000000000001fafca43cabdb0c6385daffa8a039f3b44b9b17271d7106704" => 1090800,
                "0000000000000006e9693e34fc55452c82328f31e069df740655b55dd07cb58b" => 1091016,
                "0000000000000010e7c080046121900cee1c7de7fe063c7d81405293a9764733" => 1092384,
                "0000000000000022ef41cb09a617d87c12c6841eea47310ae6a4d1e2702bb3d3" => 1090752,
                "0000000000000017705efcdaefd6a1856becc0b915de6fdccdc9e149c1ff0e8f" => 1091856,
                "0000000000000000265a9516f35dd85d32d103d4c3b95e81969a03295f46cf0c" => 1091952,
                "0000000000000002dfd994409f5b6185573ce22eae90b4a1c37003428071f0a8" => 1090968,
                "000000000000001b8d6aaa56571d987ee50fa2e2e9a28a8482de7a4b52308f25" => 1091136,
                "0000000000000020635160b49a18336031af2d25d9a37ea211d514f196220e9d" => 1090440,
                "000000000000001bfb2ac93ebe89d9831995462f965597efcc9008b2d90fd29f" => 1091784,
                "000000000000000028515b4c442c74e2af945f08ed3b66f05847022cb25bb2ec" => 1091688,
                "000000000000000ed6b9517da9a1df88d03a5904a780aba1200b474dab0e2e4a" => 1090488,
                "000000000000000b44a550a61f9751601065ff329c54d20eb306b97d163b8f8c" => 1091712,
                "000000000000001d831888fbd1899967493856c1abf7219e632b8e73f25e0c81" => 1091064,
                "00000000000000073b62bf732ab8654d27b1296801ab32b7ac630237665162a5" => 1091304,
                "0000000000000004c0b03207179143f028c07ede20354fab68c731cb02f95fc8" => 1090656,
                "000000000000000df9d9376b9c32ea640ecfac406b41445bb3a4b0ee6625e572" => 1091040,
                "00000000000000145c3e1b3bb6f53d5e2dd441ac41c3cfe48a5746c7b168a415" => 1092240,
                "000000000000000d8bf4cade14e398d69884e991591cb11ee7fec49167e4ff85" => 1092000,
                "000000000000001d098ef14fa032b33bcfc8e559351be8cd689e03c9678256a9" => 1091472,
                "0000000000000000c25139a9227273eb7547a1f558e62c545e62aeb236e66259" => 1090584,
                "0000000000000010785f105cc7c256b5365c597a9212e99beda94c6eff0647c3" => 1091376,
                "0000000000000000fafe0f7314104d81ab34ebd066601a38e5e914f2b3cefce9" => 1092552,
                "000000000000000ddbfad338961f2d900d62f1c3b725fbd72052da062704901c" => 1090848,
                "000000000000000e5d9359857518aaf3685bf8af55c675cf0d17a45383ca297f" => 1091520,
                "0000000000000012b444de0be31d695b411dcc6645a3723932cabc6b9164531f" => 1092916,
                "000000000000001c414007419fc22a2401b07ab430bf433c8cdfb8877fb6b5b7" => 1092672,
                "000000000000000355efb9a350cc76c7624bf42abea845770a5c3adc2c5b93f4" => 1092576,
                "000000000000000f327555478a9d580318cb6e15db059642eff84797bf133196" => 1091808,
                "0000000000000003b3ea97e688f1bec5f95930950b54c1bb01bf67b029739696" => 1091640,
                "000000000000001a0d96dbc0cac26e445454dd2506702eeee7df6ff35bdcf60e" => 1091544,
                "000000000000001aac60fafe05124672b19a1c3727dc17f106f11295db1053a3" => 1092288,
                "000000000000000e37bca1e08dff47ef051199f24e9104dad85014c323464069" => 1091208,
                "0000000000000013dd0059e5f701a39c0903e7f16d393f55fc896422139a4291" => 1092768,
                "000000000000000f4c8d5bdf6b89435d3a9789fce401286eb8f3f6eeb84f2a1d" => 1091160,
                "000000000000001414ff2dd44ee4c01c02e6867228b4e1ff490f635f7de949a5" => 1091232,
                "0000000000000013b130038d0599cb5a65165fc03b1b38fe2dd1a3bad6e253df" => 1092312,
                "00000000000000082cb9d6d169dc625f64a6a24756ba796eaab131a998b42910" => 1091928,
                "0000000000000001e358bce8df79c24def4787bf0bf7af25c040342fae4a18ce" => 1091880,
                _ => u32::MAX
            },
            ChainType::TestNet => match key {
                "0000000007697fd69a799bfa26576a177e817bc0e45b9fcfbf48b362b05aeff2" => 72000,
                "0000000004c19db86b34bc9b5288b5af2aaff507e8474fa2db99e1ea03bacdfe" => 122328,
                "000000000282ab23f92f5b517325e8da93ae470a9de3fe3aeebfcaa54cb48155" => 122352,
                "000000000bca30e387a942d9dbcf6ad2273ab6061c50e5dc8282c6ff73cc3c99" => 122376,
                "0000000000bee166c1c3194f50f667900319e1fd9666aef8ec4a10accfbf3df3" => 122400,
                "000000000a7c1dfff2586d2a635dd9b8ae491aae1b6ca72bc9070d1bd0cd50bc" => 122424,
                "00000000094f05e8cbf8c8fca55f688f4fbb6ec3624dbda9eab1039f005e64de" => 122448,
                "000000000b6e93b1c97696e5de41fb3e9b94fab2df5654c1c2ddad636a6a85e3" => 122472,
                "0000000003d2d2527624d1509885f0ab3d38d476d67c6fe0da7f5df8c460a675" => 122520,
                "000000000108e218babaca583a3bc69f1273e6468e7eb27078da6374cdf14bb8" => 122544,
                "000000000ce60869ccd9258c81307a71457581d4ce0f8e684aeda300a481d9a5" => 122568,
                "0000000002738de17d2db957ddbdd207d66c2e8977ba8d7d8da541b67d4eb0fa" => 122592,
                "0000000003bb193de9431c474ac0247bc20cfc2a318084329ea88fc642b554e3" => 122616,
                "0000000002ef3d706192992b6823ed1c6221a794d1225346c97c7a3d75c88b3f" => 122640,
                "00000000054437d43f5d12eaa4898d8b85e8521b1897674ee847f070045669ad" => 122664,
                "0000000002ed5b13979a23330c5e219ea530ae801293df74d38c6cd6e7be78b9" => 122688,
                "0000000003a583ca0e218394876ddce04a94274add270c24ebd21b6570b0b202" => 122712,
                "000000000525063bee5e6935224a03d160b21965bba60320802c8f3201d0ebae" => 122736,
                "000000000d201a317e82baaf536f889c83b62add5bd0375744ce1ee77e3d099f" => 122760,
                "0000000006221f59fb1bc78200724447db51545cc43ffd5a78eed78106bbdb1a" => 122784,
                "0000000015f89c20b07c7e6a5df001bd9838a1eee4d33a1468860daeab8d2ba3" => 122808,
                "0000000006cb4b5de2a176af028d859a1499a384f8c88f243f81f01bbc729c91" => 122832,
                "000000000821a7211313a614aa3f4379af7870a38740a770d7baffd3bb6578e9" => 122856,
                "0000000008e87f07d3d1abbaa196d68cd4bf7b19ef0ddb0cbbcf1eb86f7aea46" => 122880,
                "0000000009b4a670292967a9cd8da4ecad05586179a60e987a9b71b2c3ea1a58" => 122904,
                "0000000001d975dfc73df9040e894576f27f6c252f1540b1c092c80353cdb823" => 122928,
                "0000000003b852d8331f850491aeca3d91b43b3ef7af8208c82814c0e06cd75c" => 122952,
                "0000000005938a06c7e88a5cd3a950655bde3ed7046e9ffad542ad5902395d2b" => 122976,
                "000000000577855d5599ce9a89417628233a6ccf3a86b2938b191f3dfed2e63d" => 123000,
                _ => u32::MAX
            },
            _ => u32::MAX,
        }
    }

    pub fn get_file_as_byte_vec(filename: &String) -> Vec<u8> {
        let mut f = fs::File::open(&filename).expect("no file found");
        let metadata = fs::metadata(&filename).expect("unable to read metadata");
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(&mut buffer).expect("buffer overflow");
        buffer
    }

    pub fn message_from_file(name: String) -> Vec<u8> {
        let executable = env::current_exe().unwrap();
        let path = match executable.parent() {
            Some(name) => name,
            _ => panic!()
        };
        let filepath = format!("{}/../../../files/{}", path.display(), name.as_str());
        println!("{:?}", filepath);
        let file = get_file_as_byte_vec(&filepath);
        file
    }

    pub fn assert_diff_result(chain: ChainType, result: types::MNListDiffResult) {
        let mut masternode_list = unsafe { (*result.masternode_list).decode() };
        let bh = block_height_for(chain, masternode_list.block_hash.reversed().to_string().as_str());

        assert!(result.has_found_coinbase, "Did not find coinbase at height {}", bh);
        //turned off on purpose as we don't have the coinbase block
        //assert!(result.has_valid_coinbase, "Coinbase not valid at height {}", bh);
        assert!(result.has_valid_mn_list_root, "rootMNListValid not valid at height {}", bh);
        assert!(result.has_valid_llmq_list_root, "rootQuorumListValid not valid at height {}", bh);
        assert!(result.has_valid_quorums, "validQuorums not valid at height {}", bh);
    }

    pub unsafe extern "C" fn block_height_lookup_5078(_block_hash: *mut [u8; 32], _context: *const std::ffi::c_void) -> u32 {
        5078
    }
    pub unsafe extern "C" fn get_block_hash_by_height_5078(_block_height: u32, _context: *const std::ffi::c_void) -> *const u8 {
        null_mut()
    }

    pub unsafe extern "C" fn get_llmq_snapshot_by_block_height(_block_height: u32, _context: *const std::ffi::c_void) -> *const types::LLMQSnapshot {
        null_mut()
    }

    pub unsafe extern "C" fn masternode_list_lookup(_block_hash: *mut [u8; 32], _context: *const std::ffi::c_void) -> *const types::MasternodeList {
        null_mut()
    }
    pub unsafe extern "C" fn masternode_list_destroy(_masternode_list: *const types::MasternodeList) {

    }
    pub unsafe extern "C" fn add_insight_lookup(_hash: *mut [u8; 32], _context: *const std::ffi::c_void) {

    }
    pub unsafe extern "C" fn should_process_llmq_of_type(llmq_type: u8, context: *const std::ffi::c_void) -> bool {
        let data: &mut FFIContext = &mut *(context as *mut FFIContext);
        llmq_type == match data.chain {
            ChainType::MainNet => LLMQType::Llmqtype400_60.into(),
            ChainType::TestNet => LLMQType::Llmqtype50_60.into(),
            ChainType::DevNet => LLMQType::Llmqtype60_75.into()
        }
    }
    pub unsafe extern "C" fn validate_llmq_callback(data: *mut types::LLMQValidationData, _context: *const std::ffi::c_void) -> bool {
        let result = unbox_any(data);
        let types::LLMQValidationData { items, count, commitment_hash, all_commitment_aggregated_signature, threshold_signature, public_key } = *result;
        println!("validate_quorum_callback: {:?}, {}, {:?}, {:?}, {:?}, {:?}", items, count, commitment_hash, all_commitment_aggregated_signature, threshold_signature, public_key);

        // bool allCommitmentAggregatedSignatureValidated = [DSBLSKey verifySecureAggregated:commitmentHash signature:allCommitmentAggregatedSignature withPublicKeys:publicKeyArray];

        // let mut inputs = Vec::new();
        // let mut asig = AggregateSignature::new();
        let all_commitment_aggregated_signature = UInt768(*all_commitment_aggregated_signature);
        let threshold_signature = UInt768(*threshold_signature);
        let public_key = UInt384(*public_key);
        let commitment_hash = UInt256(*commitment_hash);

        let infos = (0..count)
            .into_iter()
            .map(|i|
                AggregationInfo {
                    public_key: UInt384(*(*(items.offset(i as isize)))),
                    digest: commitment_hash
                })
            .collect::<Vec<AggregationInfo>>();

        true
    }

    pub fn perform_mnlist_diff_test_for_message(
        hex_string: &str,
        should_be_total_transactions: u32,
        verify_string_hashes: Vec<&str>,
        verify_string_smle_hashes: Vec<&str>,
        chain: ChainType) {
        println!("perform_mnlist_diff_test_for_message...");
        let bytes = Vec::from_hex(&hex_string).unwrap();
        let length = bytes.len();
        let c_array = bytes.as_ptr();
        let message: &[u8] = unsafe { slice::from_raw_parts(c_array, length) };

        let merkle_root = [0u8; 32].as_ptr();
        let offset = &mut 0;
        assert!(length - *offset >= 32);
        let base_block_hash = UInt256::from_bytes(message, offset).unwrap();
        assert_ne!(base_block_hash, UInt256::default() /*UINT256_ZERO*/, "Base block hash should NOT be empty here");
        assert!(length - *offset >= 32);
        let _block_hash = UInt256::from_bytes(message, offset).unwrap();
        assert!(length - *offset >= 4);
        let total_transactions = u32::from_bytes(message, offset).unwrap();
        assert_eq!(total_transactions, should_be_total_transactions, "Invalid transaction count");
        let use_insight_as_backup = false;
        let base_masternode_list_hash: *const u8 = null_mut();
        let context = &mut FFIContext { chain } as *mut _ as *mut std::ffi::c_void;
        let result = mnl_diff_process(
            c_array,
            length,
            base_masternode_list_hash,
            merkle_root,
            use_insight_as_backup,
            |block_hash| 122088,
            |height| null_mut(),
            get_llmq_snapshot_by_block_height,
            |block_hash| null_mut(),
            masternode_list_destroy,
            add_insight_lookup,
            should_process_llmq_of_type,
            validate_llmq_callback,
            context
        );
        println!("result: {:?}", result);
        let result = unsafe { unbox_any(result) };
        let masternode_list = unsafe { (*unbox_any(result.masternode_list)).decode() };
        let masternodes = masternode_list.masternodes.clone();
        let mut pro_tx_hashes: Vec<UInt256> = masternodes.clone().into_keys().collect();
        pro_tx_hashes.sort();
        let mut verify_hashes: Vec<UInt256> = verify_string_hashes
            .into_iter()
            .map(|h|
                Vec::from_hex(h)
                    .unwrap()
                    .read_with::<UInt256>(&mut 0, byte::LE)
                    .unwrap()
                    .reversed()
            )
            .collect();
        verify_hashes.sort();
        assert_eq!(verify_hashes, pro_tx_hashes, "Provider transaction hashes");
        let mut masternode_list_hashes: Vec<UInt256> = pro_tx_hashes
            .clone()
            .iter()
            .map(|hash| masternodes[hash].entry_hash)
            .collect();
        masternode_list_hashes.sort();
        let mut verify_smle_hashes: Vec<UInt256> = verify_string_smle_hashes
            .into_iter()
            .map(|h|
                Vec::from_hex(h)
                    .unwrap()
                    .read_with::<UInt256>(&mut 0, byte::LE)
                    .unwrap())
            .collect();
        verify_smle_hashes.sort();
        assert_eq!(masternode_list_hashes, verify_smle_hashes, "SMLE transaction hashes");
        assert!(result.has_found_coinbase, "The coinbase was not part of provided hashes");
    }

    pub fn load_masternode_lists_for_files
    <'a, BlockHeightByHash: Fn(UInt256) -> u32 + Copy>(
        files: Vec<String>, chain: ChainType,
        get_block_height_by_hash: BlockHeightByHash)
        -> (bool, HashMap<UInt256, masternode::MasternodeList>) {
        let mut lists: HashMap<UInt256, masternode::MasternodeList> = HashMap::new();
        let mut base_masternode_list_hash: Option<UInt256> = None;
        for file in files {
            println!("load_masternode_lists_for_files: [{}]", file);
            let bytes = message_from_file(file);
            let result = mnl_diff_process(
                bytes.as_ptr(),
                bytes.len(),
                match base_masternode_list_hash { Some(data) => data.0.as_ptr(), None => null_mut() },
                [0u8; 32].as_ptr(),
                false,
                get_block_height_by_hash,
                |height| null_mut(),
                get_llmq_snapshot_by_block_height,
                |hash| match lists.get(&hash) {
                    Some(list) => {
                        let list_encoded = list.clone().encode();
                        &list_encoded as *const types::MasternodeList
                    },
                    None => null_mut()
                },
                masternode_list_destroy,
                add_insight_lookup,
                should_process_llmq_of_type,
                validate_llmq_callback,
                &mut (FFIContext { chain }) as *mut _ as *mut std::ffi::c_void
            );
            let result = unsafe { *result };
            println!("result: [{:?}]", result);
            //println!("MNDiff: {} added, {} modified", result.added_masternodes_count, result.modified_masternodes_count);
            assert_diff_result(chain, result);
            let block_hash = UInt256(unsafe { *result.block_hash });
            let masternode_list = unsafe { *result.masternode_list };
            let masternode_list_decoded = unsafe { masternode_list.decode() };
            base_masternode_list_hash = Some(block_hash);
            lists.insert(block_hash.clone(), masternode_list_decoded);
        }
        (true, lists)
    }
}


