use strum::{Display, FromRepr};

#[derive(Debug, Display, FromRepr)]
#[repr(u8)]
pub enum OpCode {
  Add,
  Class,
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
  GetProperty,
  GetUpvalue,
  Greater,
  Invoke,
  Jump,
  JumpIfFalse,
  Less,
  Loop,
  Method,
  Multiply,
  Negate,
  Nil,
  Not,
  Pop,
  Print,
  Return,
  SetGlobal,
  SetLocal,
  SetProperty,
  SetUpvalue,
  Subtract,
  True,
}

impl From<OpCode> for u8 {
  fn from(opcode: OpCode) -> Self {
    opcode as Self
  }
}
