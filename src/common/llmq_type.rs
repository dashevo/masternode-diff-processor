use byte::ctx::Endian;
use byte::{BytesExt, TryRead, TryWrite};
use crate::consensus::Encodable;

#[warn(non_camel_case_types)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Hash, Ord)]
pub enum LLMQType {
    Llmqtype50_60 = 1,  // every 24 blocks
    Llmqtype400_60 = 2, // 288 blocks
    Llmqtype400_85 = 3, // 576 blocks
    Llmqtype100_67 = 4, // every 24 blocks
    Llmqtype60_80 = 5, // 60 members, 48 (80%) threshold, one per hour
    Llmqtype5_60 = 100, // 24 blocks
    Llmqtype10_60 = 101, // 24 blocks
}

impl LLMQType {
    pub fn quorum_size(&self) -> u32 {
        match self {
            LLMQType::Llmqtype5_60 => 5,
            LLMQType::Llmqtype10_60 => 10,
            LLMQType::Llmqtype50_60 => 50,
            LLMQType::Llmqtype60_80 => 60,
            LLMQType::Llmqtype400_60 => 400,
            LLMQType::Llmqtype400_85 => 400,
            LLMQType::Llmqtype100_67 => 100,
        }
    }

    pub fn quorum_threshold(&self) -> u32 {
        match self {
            LLMQType::Llmqtype50_60 => 30,
            LLMQType::Llmqtype400_60 => 240,
            LLMQType::Llmqtype400_85 => 340,
            LLMQType::Llmqtype100_67 => 67,
            LLMQType::Llmqtype60_80 => 48,
            LLMQType::Llmqtype5_60 => 3,
            LLMQType::Llmqtype10_60 => 6,
        }
    }
}

impl From<u8> for LLMQType {
    fn from(orig: u8) -> Self {
        match orig {
            0x01 => LLMQType::Llmqtype50_60,
            0x02 => LLMQType::Llmqtype400_60,
            0x03 => LLMQType::Llmqtype400_85,
            0x04 => LLMQType::Llmqtype100_67,
            0x64 => LLMQType::Llmqtype5_60,
            0x65 => LLMQType::Llmqtype10_60,
            _ => LLMQType::Llmqtype50_60
        }
    }
}

impl Into<u8> for LLMQType {
    fn into(self) -> u8 {
        match self {
            LLMQType::Llmqtype50_60 => 0x01,
            LLMQType::Llmqtype400_60 => 0x02,
            LLMQType::Llmqtype400_85 => 0x03,
            LLMQType::Llmqtype100_67 => 0x04,
            LLMQType::Llmqtype60_80 => 0x05,
            LLMQType::Llmqtype5_60 => 0x64,
            LLMQType::Llmqtype10_60 => 0x65
        }
    }
}

impl<'a> TryRead<'a, Endian> for LLMQType {
    fn try_read(bytes: &'a [u8], endian: Endian) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let orig = bytes.read_with::<u8>(offset, endian).unwrap();
        let llmq_type = LLMQType::from(orig);
        Ok((llmq_type, 1))
    }
}

impl<'a> TryWrite<Endian> for LLMQType {
    fn try_write(self, bytes: &mut [u8], _endian: Endian) -> byte::Result<usize> {
        let orig: u8 = self.into();
        orig.consensus_encode(bytes).unwrap();
        Ok(1)
    }
}
