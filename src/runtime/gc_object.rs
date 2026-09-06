use std::any::type_name;
use std::slice::from_raw_parts_mut;

use crate::core::memory::Freeable;

use super::hash_table::HashTable;
use super::heap_gc::DEBUG_LOG_GC;

use super::heap_object::FunctionObj::{MainScript, UserDefined};
use super::heap_object::HeapObject::{
  self, HeapBoundMethod, HeapClass, HeapClosure, HeapFunction, HeapNativeFn, HeapObjInstance, HeapString,
  HeapUpvalue,
};

use super::value::Value;
use super::value_array::ValueArray;

pub(super) enum Blackenable {
  Array(&'static ValueArray),
  Object(&'static mut GcObject),
  Table(&'static mut HashTable),
  Value(&'static Value),
}

#[derive(Debug)]
#[repr(C)]
pub struct GcObject {
  pub next: *mut Self,
  pub object: HeapObject,
  pub(super) is_marked: bool,
}

impl GcObject {
  pub(super) fn blackenables(&self) -> Vec<Blackenable> {
    if DEBUG_LOG_GC {
      println!("{self:?} blacken {:?}", self.object);
    }

    match self.object {
      HeapBoundMethod(obj_ptr) => {
        let receiver = &(unsafe { &mut *obj_ptr }.receiver);
        let method_ref = unsafe { &mut *obj_ptr }.method_gc_mut();
        vec![Blackenable::Value(receiver), Blackenable::Object(method_ref)]
      },

      HeapClass(obj_ptr) => {
        let name_obj = unsafe { &mut *obj_ptr }.name_gc_mut();
        let methods_ref = &mut unsafe { &mut *obj_ptr }.methods;
        vec![Blackenable::Object(name_obj), Blackenable::Table(methods_ref)]
      },

      HeapClosure(obj_ptr) => {
        let closure = unsafe { &*obj_ptr };
        let function_gc = unsafe { &mut *closure.function_gc_ptr };

        let mut out = Vec::with_capacity(1 + closure.upvalue_count as usize);
        out.push(Blackenable::Object(&mut *function_gc));

        let uvps = unsafe { from_raw_parts_mut(closure.upvalues_ptr_ptr, closure.upvalue_count as usize) };

        for upvalue_ptr in uvps {
          out.push(Blackenable::Object(unsafe { &mut **upvalue_ptr }));
        }

        out
      },

      HeapFunction(obj_ptr) => {
        let function = unsafe { &*obj_ptr };
        match function {
          MainScript { chunk, .. } => vec![Blackenable::Array(&chunk.constants)],
          UserDefined { chunk, name_gc_ptr, .. } => {
            let gc_obj_ref = unsafe { &mut **name_gc_ptr };
            vec![Blackenable::Array(&chunk.constants), Blackenable::Object(gc_obj_ref)]
          },
        }
      },

      HeapObjInstance(obj_ptr) => {
        let class_gc = unsafe { &mut *obj_ptr }.class_gc_mut();
        let fields_ref = &mut (unsafe { &mut *obj_ptr }.fields);
        vec![Blackenable::Object(class_gc), Blackenable::Table(fields_ref)]
      },

      HeapUpvalue(obj_ptr) => {
        let closed_value_ref = &(unsafe { &*obj_ptr }.closed_value);
        vec![Blackenable::Value(closed_value_ref)]
      },

      HeapNativeFn(_) | HeapString(_) => Vec::new(),
    }
  }

  pub(super) fn set_marked(&mut self) {
    if DEBUG_LOG_GC {
      println!("Now marked: {self:?}");
    }
    self.is_marked = true;
  }
}

impl Freeable for GcObject {
  fn free(&mut self) {
    if DEBUG_LOG_GC {
      println!("{self:?} free type {}", type_name::<Self>());
    }
    self.object.free();
  }
}
