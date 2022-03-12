use byte::{BytesExt, check_len, LE, Result, TryRead, TryWrite};
use byte::ctx::{Bytes, Endian};
use std::fmt::Write;
use crate::consensus::{Decodable, Encodable, ReadExt, WriteExt};
use crate::consensus::encode::{Error, VarInt};
use crate::hashes::{Hash, sha256d, hex::{FromHex, ToHex}, hex};

pub trait Data {
    fn bit_is_true_at_le_index(&self, index: u32) -> bool;
    fn true_bits_count(&self) -> u64;
}

impl Data for [u8] {

    fn bit_is_true_at_le_index(&self, index: u32) -> bool {
        let offset = &mut ((index / 8) as usize);
        let bit_position = index % 8;
        match self.read_with::<u8>(offset, LE) {
            Ok(bits) => (bits >> bit_position) & 1 != 0,
            _ => false
        }
    }

    fn true_bits_count(&self) -> u64 {
        let mut count = 0;
        for mut i in 0..self.len() {
            let mut bits: u8 = self.read_with(&mut i, LE).unwrap();
            for _j in 0..8 {
                if bits & 1 != 0 {
                    count += 1;
                }
                bits >>= 1;
            }
        }
        count
    }
}


pub fn hex_with_data(data: &[u8]) -> String {
    let n = data.len();
    let mut s = String::with_capacity(2 * n);
    let mut iter = data.iter();
    while let Some(a) = iter.next() {
        write!(s, "{:02X}", a).unwrap();
    }
    s
}

#[inline]
pub fn data_at_offset_from<'a>(buffer: &'a [u8], offset: &mut usize) -> Result<&'a [u8]> {
    let var_int: VarInt = match VarInt::consensus_decode(&buffer[*offset..]) {
        Ok(data) => data,
        Err(_error) => {
            return byte::Result::Err(byte::Error::Incomplete);
        }
    };
    let var_int_value = var_int.0 as usize;
    let var_int_length = var_int.len();
    *offset += var_int_length;
    let data: &[u8] = match buffer.read_with(offset, Bytes::Len(var_int_value)) {
        Ok(data) => data,
        Err(error) => { return byte::Result::Err(error); }
    };
    Ok(data)
}

pub fn merkle_root_from_hashes(hashes: Vec<UInt256>) -> Option<UInt256> {
    let length = hashes.len();
    let mut level = hashes.clone();
    if length == 0 { return None; }
    if length == 1 { return Some(hashes[0]); }
    let mut higher_level: Vec<UInt256> = vec![];
    while level.len() != 1 {
        for i in (0..level.len()).step_by(2) {
            let left = level[i];
            let offset = &mut 0;
            let mut buffer: Vec<u8> = Vec::with_capacity(64);
            *offset += left.consensus_encode(&mut buffer).unwrap();
            *offset +=
                if level.len() - i > 1 {
                    level[i+1]
                } else {
                    left
                }.consensus_encode(&mut buffer).unwrap();

            higher_level.push(UInt256(sha256d::Hash::hash(&buffer).into_inner()));
        }
        level = higher_level.clone();
        higher_level.clear();
    }
    return Some(level[0]);
}

pub fn short_hex_string_from(data: &[u8]) -> String {
    let hex_data = hex_with_data(data);
    if hex_data.len() > 7 {
        hex_data[..7].to_string()
    } else {
        hex_data
    }
}

impl<'a> TryWrite for &'a VarInt {
    #[inline]
    fn try_write(self, bytes: &mut [u8], _ctx: ()) -> Result<usize> {
        check_len(bytes, self.len())?;
        Ok(match self.consensus_encode(bytes) {
            Ok(size) => size,
            _ => 0
        })
    }
}

pub const MN_ENTRY_PAYLOAD_LENGTH: usize = 151;

pub trait Reversable {
    fn reversed(&mut self) -> Self;
}

pub trait Zeroable {
    fn is_zero(&self) -> bool;
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UInt128(pub [u8; 16]);
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UInt160(pub [u8; 20]);
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UInt256(pub [u8; 32]);
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UInt384(pub [u8; 48]);
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UInt768(pub [u8; 96]);
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MNPayload(pub [u8; MN_ENTRY_PAYLOAD_LENGTH]);

macro_rules! define_bytes_to_big_uint {
    ($uint_type: ident, $byte_len: expr) => {
        impl<'a> TryRead<'a, Endian> for $uint_type {
            fn try_read(bytes: &'a [u8], endian: Endian) -> Result<(Self, usize)> {
                let offset = &mut 0;
                let mut data: [u8; $byte_len] = [0u8; $byte_len];
                for _i in 0..$byte_len {
                    let index = offset.clone();
                    let chunk = match bytes.read_with::<u8>(offset, endian) {
                        Ok(data) => data,
                        Err(_err) => { return Err(_err); }
                    };
                    data[index] = chunk;
                }
                Ok(($uint_type(data), $byte_len))
            }
        }
        impl std::default::Default for $uint_type {
            fn default() -> Self {
                let mut data: [u8; $byte_len] = [0u8; $byte_len];
                for i in 0..$byte_len {
                    data[i] = 0;
                }
                Self(data)
            }
        }

        impl std::fmt::Display for $uint_type {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.0.to_hex())?;
                Ok(())
            }
        }
        impl std::fmt::Debug for $uint_type {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                // for i in 0..self.len() {
                    write!(f, "{}", self.0.to_hex())?;
                // }
                Ok(())
            }
        }
        impl Encodable for $uint_type {
            #[inline]
            fn consensus_encode<W: std::io::Write>(&self, mut writer: W) -> std::result::Result<usize, std::io::Error> {
                writer.emit_slice(&self.0[..])?;
                Ok($byte_len)
            }
        }

        impl Decodable for $uint_type {
            #[inline]
            fn consensus_decode<D: std::io::Read>(mut d: D) -> std::result::Result<Self, Error> {
                let mut ret = [0; $byte_len];
                d.read_slice(&mut ret)?;
                Ok($uint_type(ret))
            }
        }

        impl Reversable for $uint_type {
            fn reversed(&mut self) -> Self {
                self.0.reverse();
                *self
            }
        }
        impl FromHex for $uint_type {
            fn from_byte_iter<I>(iter: I) -> std::result::Result<Self, hex::Error>
                where I: Iterator<Item=std::result::Result<u8, hashes::hex::Error>> +
                ExactSizeIterator +
                DoubleEndedIterator {
                if iter.len() == $byte_len {
                    let mut ret = [0; $byte_len];
                    for (n, byte) in iter.enumerate() {
                        ret[n] = byte?;
                    }
                    Ok($uint_type(ret))
                } else {
                    Err(hex::Error::InvalidLength(2 * $byte_len, 2 * iter.len()))
                }
            }
        }

        impl Zeroable for $uint_type {
            fn is_zero(&self) -> bool {
                for i in 0..$byte_len {
                    if self.0[i] > 0 {
                        return false;
                    }
                }
                true
            }
            // fn is_zero(&self) -> bool {
            //     for i in 0..$byte_len {
            //         if self.0[i] == 1 {
            //             return true;
            //         }
            //     }
            //     false
            // }
        }
    }
}

define_bytes_to_big_uint!(UInt128, 16);
define_bytes_to_big_uint!(UInt160, 20);
define_bytes_to_big_uint!(UInt256, 32);
define_bytes_to_big_uint!(UInt384, 48);
define_bytes_to_big_uint!(UInt768, 96);
define_bytes_to_big_uint!(MNPayload, MN_ENTRY_PAYLOAD_LENGTH);
