use byte::{BytesExt, LE};
use secrets::traits::AsContiguousBytes;
use crate::consensus::Decodable;
use crate::consensus::encode::VarInt;
use crate::crypto::byte_util::UInt256;
use crate::hashes::{Hash, sha256d};
use crate::transactions::transaction::{Transaction};
use crate::transactions::transaction::TransactionType::{Coinbase};

// #[repr(C)]
// #[derive(Debug)]
pub struct CoinbaseTransaction<'a> {
    pub base: Transaction<'a>,
    pub coinbase_transaction_version: u16,
    pub height: u32,
    pub merkle_root_mn_list: UInt256,
    pub merkle_root_llmq_list: Option<UInt256>,
}

impl CoinbaseTransaction<'_> {
    pub fn new(message: &'static [u8]) -> Option<Self> {
        if let Some(mut base) = Transaction::new(message) {
            base.tx_type = Coinbase;
            let mut offset = base.payload_offset;
            let extra_payload_size = match VarInt::consensus_decode(&message[*offset..]) {
                Ok(data) => data,
                Err(_err) => { return None; }
            };
            *offset += extra_payload_size.len();
            let coinbase_transaction_version = match message.read_with::<u16>(offset, LE) {
                Ok(data) => data,
                Err(_err) => { return None; }
            };
            let height = match message.read_with::<u32>(offset, LE) {
                Ok(data) => data,
                Err(_err) => { return None; }
            };
            let merkle_root_mn_list = match message.read_with::<UInt256>(offset, LE) {
                Ok(data) => data,
                Err(_err) => { return None; }
            };
            let merkle_root_llmq_list: Option<UInt256> =
                if coinbase_transaction_version == 2 {
                    match message.read_with::<UInt256>(offset, LE) {
                        Ok(data) => Some(data),
                        Err(_err) => { return None; }
                    }
                } else { None };

            base.payload_offset = offset;
            base.tx_hash = Some(UInt256(sha256d::Hash::hash(&base.to_data()).into_inner()));
            return Some(Self {
                base,
                coinbase_transaction_version,
                height,
                merkle_root_mn_list,
                merkle_root_llmq_list
            });
        }
        None
    }

    fn payload_data(&self) -> &[u8] {
        let mut buf = [0u8];
        buf[0..15].copy_from_slice(&self.coinbase_transaction_version.as_bytes());
        buf[16..47].copy_from_slice(&self.height.as_bytes());
        buf[48..303].copy_from_slice(self.merkle_root_mn_list.0.as_bytes());
        if self.coinbase_transaction_version >= 2 {
            if let Some(llmq_list) = self.merkle_root_llmq_list {
                buf[304..559].copy_from_slice(llmq_list.0.as_bytes());
            }
        }
        &buf
    }
}
