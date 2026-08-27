use strum::{Display, FromRepr};

#[derive(Debug, Display, FromRepr)]
#[repr(u8)]
pub enum OpCode {
  Add,
  CloseUpvalue,
  Closure,
  Constant,
  DefineGlobal,
  Divide,
  Equal,
  False,
  FnCall,
  GetGlobal,
  GetLocal,
  GetUpvalue,
  Greater,
  Jump,
  JumpIfFalse,
  Less,
  Loop,
  Multiply,
  Negate,
  Nil,
  Not,
  Pop,
  Print,
  Return,
  SetGlobal,
  SetLocal,
  SetUpvalue,
  Subtract,
  True,
}

impl From<OpCode> for u8 {
  fn from(opcode: OpCode) -> Self {
    opcode as Self
  }
}
