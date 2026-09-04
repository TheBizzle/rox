use std::ptr::null_mut;
use std::slice::from_raw_parts_mut;

use crate::memory::{free_array, grow_array, next_capacity};

use crate::gc::GcObject;
use crate::gc::HeapObject::{
  HeapBoundMethod, HeapClass, HeapClosure, HeapFunction, HeapNativeFn, HeapObjInstance, HeapString,
  HeapUpvalue,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
  Boolean(bool),
  Double(f64),
  Nil,
  Reference(*mut GcObject),
}

impl Value {
  #[must_use]
  pub fn stringify(&self) -> String {
    match self {
      Self::Double(x) => format!("{x}"),
      Self::Boolean(true) => "true".to_string(),
      Self::Boolean(false) => "false".to_string(),
      Self::Nil => "nil".to_string(),
      Self::Reference(gc_ptr) => match unsafe { &**gc_ptr }.object {
        HeapBoundMethod(bound_method_ptr) => unsafe { &*bound_method_ptr }.to_string(),
        HeapClass(class_obj_ptr) => unsafe { &*class_obj_ptr }.to_string(),
        HeapClosure(fn_obj_ptr) => unsafe { &*fn_obj_ptr }.to_string(),
        HeapFunction(fn_ptr) => unsafe { &*fn_ptr }.to_string(),
        HeapNativeFn(native_fn_ptr) => unsafe { &*native_fn_ptr }.to_string(),
        HeapObjInstance(obj_instance_ptr) => unsafe { &*obj_instance_ptr }.to_string(),
        HeapString(str_ptr) => unsafe { &*str_ptr }.to_string(),
        HeapUpvalue(upvalue_ptr) => unsafe { &*upvalue_ptr }.to_string(),
      },
    }
  }
}

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
