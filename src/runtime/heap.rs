use std::ptr::null_mut;

use crate::core::memory::Freeable;

use super::gc_object::GcObject;
use super::hash_table::HashTable;

pub struct Heap {
  pub globals: HashTable,
  pub(super) grays: Vec<*const GcObject>,
  pub head_open_upvalue_gc_opt: Option<*mut GcObject>,
  pub(super) objects: *mut GcObject,
  pub(super) strings: HashTable,

  pub init_str_gc_ptr: *mut GcObject,
  pub super_str_gc_ptr: *mut GcObject,
  pub this_str_gc_ptr: *mut GcObject,
}

impl Freeable for Heap {
  fn free(&mut self) {
    let mut ptr = self.objects;
    while !ptr.is_null() {
      let next = unsafe { (*ptr).next };
      unsafe {
        (*ptr).free();
        drop(Box::from_raw(ptr));
      }
      ptr = next;
    }
    self.objects = null_mut();

    self.globals.free();
    self.grays.clear();
    self.strings.free();
  }
}

impl Heap {
  pub fn new() -> Self {
    let mut this = Self {
      globals: HashTable::new(),
      grays: Vec::new(),
      head_open_upvalue_gc_opt: None,
      objects: null_mut(),
      strings: HashTable::new(),
      init_str_gc_ptr: null_mut(),
      this_str_gc_ptr: null_mut(),
      super_str_gc_ptr: null_mut(),
    };

    // TODO: Just put these in a hashmap
    this.init_str_gc_ptr = intern(&mut this, "init");
    this.this_str_gc_ptr = intern(&mut this, "this");
    this.super_str_gc_ptr = intern(&mut this, "super");

    this
  }
}

fn intern(this: &mut Heap, string: &str) -> *mut GcObject {
  this.copy_string_simple(string, string.len()).1
}
