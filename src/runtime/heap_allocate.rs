use std::alloc::{Layout, alloc, alloc_zeroed, handle_alloc_error};

use std::any::type_name;
use std::ptr::copy_nonoverlapping;

use crate::core::memory::free_array;
use crate::core::source_loc::SourceLoc;

use super::gc_object::GcObject;
use super::heap::Heap;
use super::heap_gc::DEBUG_LOG_GC;

use super::heap_object::HeapObject::{
  self, HeapBoundMethod, HeapClass, HeapClosure, HeapFunction, HeapNativeFn, HeapObjInstance, HeapString,
  HeapUpvalue,
};

use super::heap_object::{
  BoundMethodObj, ClassObj, ClosureObj, FunctionObj, NativeFnObj, ObjInstanceObj, StringObj, UpvalueObj,
};

use super::value::Value::Nil;

type GcPtr = *mut GcObject;
type StrPtr = *mut StringObj;

impl Heap {
  pub fn allocate_bound_method(&mut self, bound_method_obj: BoundMethodObj) -> GcPtr {
    self.allocate_on_heap(bound_method_obj, HeapBoundMethod).1
  }

  pub fn allocate_class(&mut self, class_obj: ClassObj) -> GcPtr {
    self.allocate_on_heap(class_obj, HeapClass).1
  }

  #[allow(clippy::cast_ptr_alignment)]
  pub fn allocate_closure(&mut self, function_gc_ptr: *mut GcObject) -> (*mut ClosureObj, GcPtr) {
    let HeapFunction(function_obj_ptr) = unsafe { &*function_gc_ptr }.object else {
      panic!("Illegal for closure's function pointer to be to a non-function");
    };
    let function_obj = unsafe { &*function_obj_ptr };
    let upvalue_count = function_obj.upvalue_count();
    let layout = Layout::array::<*mut UpvalueObj>(upvalue_count as usize).unwrap();
    let upvalues_ptr_ptr = unsafe { alloc_zeroed(layout).cast::<*mut GcObject>() };

    let closure_obj = ClosureObj { function_gc_ptr, upvalues_ptr_ptr, upvalue_count };
    self.allocate_on_heap(closure_obj, HeapClosure)
  }

  pub fn allocate_function(&mut self, function_obj: FunctionObj) -> GcPtr {
    self.allocate_on_heap(function_obj, HeapFunction).1
  }

  pub fn allocate_native_fn(&mut self, native_fn_obj: NativeFnObj) -> GcPtr {
    self.allocate_on_heap(native_fn_obj, HeapNativeFn).1
  }

  pub fn allocate_obj_instance(&mut self, obj_instance_obj: ObjInstanceObj) -> GcPtr {
    self.allocate_on_heap(obj_instance_obj, HeapObjInstance).1
  }

  fn allocate_string(&mut self, chars: *const u8, length: usize, hash: u32) -> (StrPtr, GcPtr) {
    let string_obj = StringObj { chars, length, hash };
    let (string_ptr, gc_ptr) = self.allocate_on_heap(string_obj, HeapString);
    self.strings.set(gc_ptr, Nil);
    (string_ptr, gc_ptr)
  }

  pub fn allocate_upvalue(&mut self, upvalue_obj: UpvalueObj) -> (*mut UpvalueObj, GcPtr) {
    self.allocate_on_heap(upvalue_obj, HeapUpvalue)
  }

  fn allocate_on_heap<T, F: Fn(*mut T) -> HeapObject>(&mut self, obj: T, constructor: F) -> (*mut T, GcPtr) {
    let ptr = Box::into_raw(Box::new(obj));

    if DEBUG_LOG_GC {
      let object = constructor(ptr);
      let gc_object = GcObject { next: self.objects, object: object.clone(), is_marked: false };
      println!("{object:?} allocate {} for {:?}", std::mem::size_of_val(&gc_object), type_name::<T>());
    }

    let object = constructor(ptr);
    let gc_object = GcObject { next: self.objects, object, is_marked: false };
    let gc_ptr = Box::into_raw(Box::new(gc_object));
    self.objects = gc_ptr;
    (ptr, gc_ptr)
  }

  pub fn concatenate_strings(&mut self, string1: StrPtr, string2: StrPtr) -> (StrPtr, GcPtr) {
    let str1 = unsafe { &*string1 };
    let str2 = unsafe { &*string2 };

    let length1 = str1.length;
    let length2 = str2.length;
    let length = length1 + length2;

    let layout = Layout::array::<u8>(length).unwrap();
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
      handle_alloc_error(layout);
    }

    unsafe {
      copy_nonoverlapping(str1.chars, ptr, length1);
      copy_nonoverlapping(str2.chars, ptr.add(length1), length2);
    }

    let hash = hash_string(ptr, length);
    #[allow(clippy::option_if_let_else)]
    if let Some(ptr_pair) = self.find_string(ptr, length, hash) {
      unsafe {
        free_array!(u8, ptr, length);
      }
      ptr_pair
    } else {
      self.allocate_string(ptr, length, hash)
    }
  }

  pub fn copy_string(&mut self, str: &str, loc: &SourceLoc) -> (StrPtr, GcPtr) {
    self.copy_string_simple(&loc.extract(str), loc.length as usize)
  }

  pub fn copy_string_simple(&mut self, substring: &str, length: usize) -> (StrPtr, GcPtr) {
    let hash = hash_string(substring.as_ptr(), length);
    #[allow(clippy::option_if_let_else)]
    if let Some(ptr_pair) = self.find_string(substring.as_ptr(), length, hash) {
      ptr_pair
    } else {
      let layout = Layout::array::<u8>(length).unwrap();
      let ptr = unsafe { alloc(layout) };
      if ptr.is_null() {
        eprintln!("Reallocation for characters failed");
        handle_alloc_error(layout);
      }
      unsafe {
        copy_nonoverlapping(substring.as_ptr(), ptr, length);
      }
      self.allocate_string(ptr, length, hash)
    }
  }

  fn find_string(&self, chars_ptr: *const u8, length: usize, hash: u32) -> Option<(StrPtr, GcPtr)> {
    self.strings.find_string(chars_ptr, length, hash).map(|interned_gc_ptr| {
      if let HeapString(str_ptr) = unsafe { &*interned_gc_ptr }.object {
        (str_ptr, interned_gc_ptr.cast_mut())
      } else {
        panic!("The only objects that tables can use as keys are strings")
      }
    })
  }
}

fn hash_string(chars: *const u8, length: usize) -> u32 {
  let mut hash = 2_166_136_261;
  for i in 0..length {
    hash ^= u32::from(unsafe { *chars.add(i) });
    hash = hash.wrapping_mul(16_777_619);
  }
  hash
}
