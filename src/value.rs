use std::ptr::null_mut;

use crate::memory::{free_array, grow_array, next_capacity};

use crate::gc::HeapObject::{HeapClosure, HeapFunction, HeapNativeFn, HeapString, HeapUpvalue};
use crate::gc::Reference;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
  Boolean(bool),
  Double(f64),
  Nil,
  ReferenceValue(Reference),
}

impl Value {
  #[must_use]
  pub fn stringify(&self) -> String {
    match self {
      Self::Double(x) => format!("{x}"),
      Self::Boolean(true) => "true".to_string(),
      Self::Boolean(false) => "false".to_string(),
      Self::Nil => "nil".to_string(),
      Self::ReferenceValue(Reference(heap_object)) => match heap_object {
        HeapClosure(fn_obj_ptr) => unsafe { &**fn_obj_ptr }.to_string(),
        HeapFunction(fn_ptr) => unsafe { &**fn_ptr }.to_string(),
        HeapNativeFn(native_fn_ptr) => unsafe { &**native_fn_ptr }.to_string(),
        HeapString(str_ptr) => unsafe { &**str_ptr }.to_string(),
        HeapUpvalue(upvalue_ptr) => unsafe { &**upvalue_ptr }.to_string(),
      },
    }
  }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ValueArray {
  pub count: u8,
  capacity: u8,
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
