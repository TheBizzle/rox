use std::ptr::null_mut;
use std::slice::from_raw_parts_mut;

use crate::core::memory::{free_array, grow_array, next_capacity};

use super::value::Value;

#[derive(Debug, Eq, PartialEq)]
pub struct ValueArray {
  pub count: u16,
  capacity: u16,
  pub values: *mut Value,
}

impl ValueArray {
  #[must_use]
  pub const fn default() -> Self {
    Self { count: 0, capacity: 0, values: null_mut() }
  }

  #[must_use]
  pub fn free(&mut self) -> &Self {
    unsafe {
      free_array!(Value, self.values, self.capacity);
    }

    self.count = 0;
    self.capacity = 0;
    self.values = null_mut();

    self
  }

  pub fn iter(&self) -> impl Iterator<Item = &Value> {
    if self.values.is_null() {
      [].iter()
    } else {
      unsafe { from_raw_parts_mut(self.values, self.count as usize) }.iter()
    }
  }

  pub fn write(&mut self, value: Value) {
    if self.capacity < self.count + 1 {
      let old_capacity = self.capacity;
      self.capacity = next_capacity!(old_capacity);
      self.values = unsafe { grow_array!(Value, self.values, old_capacity, self.capacity) };
    }

    unsafe {
      *self.values.add(self.count as usize) = value;
    }

    self.count += 1;
  }
}
