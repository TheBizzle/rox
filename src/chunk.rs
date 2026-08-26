use std::ptr::null_mut;

use crate::memory::{free_array, grow_array, next_capacity};
use crate::value::{Value, ValueArray};

#[derive(Debug, Eq, PartialEq)]
pub struct Chunk {
  pub count: usize,
  capacity: usize,
  pub constants: ValueArray,
  pub line_nums: *mut u32,
  pub op_codes: *mut u8,
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

  pub fn add_constant(&mut self, value: Value) -> u8 {
    self.constants.write(value);
    self.constants.count - 1
  }

  #[must_use]
  pub fn free(&mut self) -> &Self {
    unsafe {
      free_array!(u8, self.op_codes, self.capacity);
      free_array!(u32, self.line_nums, self.capacity);
    }
    let _ = self.constants.free();

    self.count = 0;
    self.capacity = 0;
    self.constants = ValueArray::default();
    self.line_nums = null_mut();
    self.op_codes = null_mut();

    self
  }

  pub fn write<T: Into<u8>>(&mut self, byte: T, line_num: u32) {
    if self.capacity < self.count + 1 {
      let old_capacity = self.capacity;
      self.capacity = next_capacity!(old_capacity);
      self.op_codes = unsafe { grow_array!(u8, self.op_codes, old_capacity, self.capacity) };
      self.line_nums = unsafe { grow_array!(u32, self.line_nums, old_capacity, self.capacity) };
    }

    unsafe {
      *self.op_codes.add(self.count) = byte.into();
      *self.line_nums.add(self.count) = line_num;
    }

    self.count += 1;
  }
}
