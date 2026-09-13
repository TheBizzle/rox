use std::ptr::null_mut;

use crate::runtime::byte::Byte;
use crate::runtime::gc_object::GcObject;
use crate::runtime::heap_object::{ClosureObj, HeapObject::HeapClosure};
use crate::runtime::value::Value;

#[derive(Debug)]
#[allow(clippy::struct_field_names)]
pub struct CallFrame {
  pub(super) closure_gc_ptr: *mut GcObject,
  pub(super) inst_ptr: *mut Byte,
  pub(super) slots_ptr: *mut Value,
}

impl CallFrame {
  pub(super) fn closure(&self) -> &ClosureObj {
    if let HeapClosure(closure_ptr) = unsafe { &*self.closure_gc_ptr }.object {
      unsafe { &*closure_ptr }
    } else {
      panic!("VM's `closure_gc_ptr` must be a closure!");
    }
  }
}

impl Default for CallFrame {
  fn default() -> Self {
    Self { closure_gc_ptr: null_mut(), inst_ptr: null_mut(), slots_ptr: null_mut() }
  }
}
