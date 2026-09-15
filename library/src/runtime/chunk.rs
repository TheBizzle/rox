use std::ptr::null_mut;

use crate::core::memory::{free_array, grow_array, next_capacity};

use super::byte::Byte;
use super::value::Value;
use super::value_array::ValueArray;

#[derive(Debug, Eq, PartialEq)]
pub struct Chunk {
  pub count: usize,
  capacity: usize,
  pub constants: ValueArray,
  pub line_nums: *mut u32,
  pub op_codes: *mut Byte,
}

impl Chunk {
  #[must_use]
  pub const fn default() -> Self {
    Self {
      count: 0,
      capacity: 0,
      constants: ValueArray::default(),
      line_nums: null_mut(),
      op_codes: null_mut(),
    }
  }

  /// # Panics
  ///
  /// When there are more than `u8::MAX` constants
  pub fn add_constant(&mut self, value: Value) -> u8 {
    self.constants.write(value);
    u8::try_from(self.constants.count - 1).unwrap()
  }

  #[must_use]
  pub fn free(&mut self) -> &Self {
    unsafe {
      free_array(self.op_codes, self.capacity);
      free_array(self.line_nums, self.capacity);
    }
    let _ = self.constants.free();

    self.count = 0;
    self.capacity = 0;
    self.constants = ValueArray::default();
    self.line_nums = null_mut();
    self.op_codes = null_mut();

    self
  }

  pub fn write(&mut self, byte: Byte, line_num: u32) {
    if self.capacity < self.count + 1 {
      let old_capacity = self.capacity;
      self.capacity = next_capacity(old_capacity);
      self.op_codes = unsafe { grow_array(self.op_codes, old_capacity, self.capacity) };
      self.line_nums = unsafe { grow_array(self.line_nums, old_capacity, self.capacity) };
    }

    unsafe {
      *self.op_codes.add(self.count) = byte;
      *self.line_nums.add(self.count) = line_num;
    }

    self.count += 1;
  }
}
