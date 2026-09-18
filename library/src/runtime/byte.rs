use crate::core::opcode::OpCode;

#[derive(Clone, Copy, Debug)]
pub enum Byte {
  Named(OpCode),
  Raw(u8),
}

impl Byte {
  #[inline]
  pub const fn as_u8(self) -> u8 {
    match self {
      Self::Named(opcode) => opcode as u8,
      Self::Raw(num) => num,
    }
  }

  #[inline]
  pub const fn as_u16(self) -> u16 {
    match self {
      Self::Named(opcode) => opcode as u16,
      Self::Raw(num) => num as u16,
    }
  }

  #[inline]
  pub const fn as_usize(self) -> usize {
    match self {
      Self::Named(opcode) => opcode as usize,
      Self::Raw(num) => num as usize,
    }
  }
}
