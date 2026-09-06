use std::ptr;

use crate::core::memory::Freeable;

use super::gc_object::{Blackenable, GcObject};
use super::hash_table::HashTable;
use super::heap::Heap;
use super::value::Value::{self, Reference};
use super::value_array::ValueArray;

pub const DEBUG_STRESS_GC: bool = false;
pub const DEBUG_LOG_GC: bool = false;

impl Heap {
  // TODO: They should just hand me a Blackenable, and I figure it out from there
  pub fn mark_array(&mut self, values: &ValueArray) {
    for value in values.iter() {
      self.mark_value(value);
    }
  }

  pub fn mark_object(&mut self, gc_object: &mut GcObject) {
    if !gc_object.is_marked {
      gc_object.set_marked();
      self.grays.push(ptr::from_ref(gc_object));
    }
  }

  pub fn mark_roots(&mut self) {
    self.mark_tables();

    for gc_ptr in [self.init_str_gc_ptr, self.super_str_gc_ptr, self.this_str_gc_ptr] {
      let str_gc = unsafe { &mut *gc_ptr };
      self.mark_object(str_gc);
    }
  }

  fn mark_table_pairs(&mut self, pairs: Vec<(*mut GcObject, *mut Value)>) {
    for (key_ptr, value_ptr) in pairs {
      self.mark_object(unsafe { &mut *key_ptr });
      self.mark_value(unsafe { &*value_ptr });
    }
  }

  fn mark_table(&mut self, table: &mut HashTable) {
    let pairs = table.iter_mut().map(|(k, v)| (ptr::from_mut(k), ptr::from_mut(v))).collect();
    self.mark_table_pairs(pairs);
  }

  fn mark_tables(&mut self) {
    let pairs = self.globals.iter_mut().map(|(k, v)| (ptr::from_mut(k), ptr::from_mut(v))).collect();
    self.mark_table_pairs(pairs);
  }

  pub fn mark_value(&mut self, value: &Value) {
    if let Reference(gc_ptr) = value {
      self.mark_object(unsafe { &mut **gc_ptr });
    }
  }

  pub fn sweep(&mut self) {
    self.strings.delete_whites();

    let mut prev_opt = None;
    let mut current_ptr = self.objects;

    while !current_ptr.is_null() {
      let this_ptr = current_ptr;
      let gc_obj = unsafe { &mut *current_ptr };
      current_ptr = gc_obj.next;
      if gc_obj.is_marked {
        gc_obj.is_marked = false;
        prev_opt = Some(gc_obj);
      } else {
        if let Some(ref mut prev) = prev_opt {
          prev.next = current_ptr;
        } else {
          self.objects = current_ptr;
        }
        gc_obj.free();
        unsafe {
          drop(Box::from_raw(this_ptr));
        }
      }
    }
  }

  pub fn trace_references(&mut self) {
    while let Some(next_gc_ptr) = self.grays.pop() {
      let next_gc_obj = unsafe { &*next_gc_ptr };
      let blackenables = next_gc_obj.blackenables();
      for bable in blackenables {
        match bable {
          Blackenable::Array(array) => {
            self.mark_array(array);
          },
          Blackenable::Object(obj) => {
            self.mark_object(obj);
          },
          Blackenable::Table(table) => {
            self.mark_table(table);
          },
          Blackenable::Value(value) => {
            self.mark_value(value);
          },
        }
      }
    }
  }
}
