use strum::{Display, FromRepr};

#[derive(Debug, Display, FromRepr)]
#[repr(u8)]
pub enum OpCode {
  Add,
  Constant,
  Divide,
  Equal,
  False,
  Greater,
  Less,
  Multiply,
  Negate,
  Nil,
  Not,
  Return,
  Subtract,
  True,
}

impl From<OpCode> for u8 {
  fn from(opcode: OpCode) -> Self {
    opcode as Self
  }
}
