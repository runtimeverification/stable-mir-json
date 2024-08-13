extern crate stable_mir;
use stable_mir::target::{Endian, MachineInfo};

const ZERO_BYTES: [u8;16] = [0u8;   16];
const SIGN_BYTES: [u8;16] = [255u8; 16];

pub fn sign_extend_128(mut bytes: Vec<u8>, signed: bool) -> Vec<u8> {
  assert!(0 < bytes.len() && bytes.len() <= 16);

  let le = MachineInfo::target_endianness() == Endian::Little;
  let needed = 16 - bytes.len();
  let sign_bit = bytes[if le { bytes.len() - 1 } else { 0 }] & 128u8 == 1u8;
  let fill_bytes = if sign_bit && signed { SIGN_BYTES } else { ZERO_BYTES };

  if needed == 0 {
      bytes
  } else if le {
      bytes.extend(fill_bytes[..needed].iter().cloned());
      bytes
  } else {
      bytes.splice(0..0, fill_bytes[..needed].iter().cloned());
      bytes
  }
}

pub fn read_u128(bytes: Vec<u8>) -> u128 {
  let bytes = sign_extend_128(bytes, false);
  assert!(bytes.len() == 16);
  if MachineInfo::target_endianness() == Endian::Little {
    u128::from_le_bytes(bytes.as_slice().try_into().unwrap())
  } else {
    u128::from_be_bytes(bytes.as_slice().try_into().unwrap())
  }
}

pub fn read_i128(bytes: Vec<u8>) -> i128 {
  let bytes = sign_extend_128(bytes, true);
  assert!(bytes.len() == 16);
  if MachineInfo::target_endianness() == Endian::Little {
    i128::from_le_bytes(bytes.as_slice().try_into().unwrap())
  } else {
    i128::from_be_bytes(bytes.as_slice().try_into().unwrap())
  }
}

pub trait FromBits { fn from_bits(v: u128) -> Self; }
impl FromBits for f16  { fn from_bits(v: u128) -> Self { f16::from_bits(v as u16) } }
impl FromBits for f32  { fn from_bits(v: u128) -> Self { f32::from_bits(v as u32) } }
impl FromBits for f64  { fn from_bits(v: u128) -> Self { f64::from_bits(v as u64) } }
impl FromBits for f128 { fn from_bits(v: u128) -> Self { f128::from_bits(v)       } }

pub fn read_float<T:FromBits>(bytes: Vec<u8>) -> T {
  T::from_bits(read_u128(bytes))
}
