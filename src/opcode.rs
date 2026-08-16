use strum::{Display, FromRepr};

#[derive(Debug, Display, FromRepr)]
#[repr(u8)]
pub enum OpCode {
  Add,
  Constant,
  Divide,
  Multiply,
  Negate,
  Return,
  Subtract,
}
