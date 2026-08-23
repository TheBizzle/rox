use strum::{Display, FromRepr};

#[derive(Debug, Display, FromRepr)]
#[repr(u8)]
pub enum OpCode {
  Add,
  Constant,
  DefineGlobal,
  Divide,
  Equal,
  False,
  GetGlobal,
  Greater,
  Less,
  Multiply,
  Negate,
  Nil,
  Not,
  Pop,
  Print,
  Return,
  SetGlobal,
  Subtract,
  True,
}

impl From<OpCode> for u8 {
  fn from(opcode: OpCode) -> Self {
    opcode as Self
  }
}
