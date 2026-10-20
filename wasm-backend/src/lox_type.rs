use strum::{EnumCount, EnumIter, FromRepr};

#[derive(Clone, Debug, EnumCount, EnumIter, FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum LoxType {
  Nil,
  Boolean,
  Number,
  Reference,
}
