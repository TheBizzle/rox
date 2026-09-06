use std::alloc::{Layout, dealloc};
use std::fmt::{Debug, Display, Formatter, Result};
use std::slice::from_raw_parts;

use crate::core::memory::Freeable;

use super::chunk::Chunk;
use super::gc_object::GcObject;
use super::hash_table::HashTable;
use super::heap::Heap;
use super::value::Value;

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HeapObject {
  HeapBoundMethod(*mut BoundMethodObj),
  HeapClass(*mut ClassObj),
  HeapClosure(*mut ClosureObj),
  HeapFunction(*mut FunctionObj),
  HeapNativeFn(*mut NativeFnObj),
  HeapObjInstance(*mut ObjInstanceObj),
  HeapString(*mut StringObj),
  HeapUpvalue(*mut UpvalueObj),
}
use HeapObject::{
  HeapBoundMethod, HeapClass, HeapClosure, HeapFunction, HeapNativeFn, HeapObjInstance, HeapString,
  HeapUpvalue,
};

impl Freeable for HeapObject {
  fn free(&mut self) {
    let (ptr, layout) = match &self {
      HeapBoundMethod(bound_method_ptr) => {
        unsafe { &mut **bound_method_ptr }.free();
        (bound_method_ptr.cast::<u8>(), Layout::new::<ClassObj>())
      },
      HeapClass(class_ptr) => {
        unsafe { &mut **class_ptr }.free();
        (class_ptr.cast::<u8>(), Layout::new::<ClassObj>())
      },
      HeapClosure(function_obj_ptr) => {
        unsafe { &mut **function_obj_ptr }.free();
        (function_obj_ptr.cast::<u8>(), Layout::new::<ClosureObj>())
      },
      HeapFunction(function_ptr) => {
        unsafe { &mut **function_ptr }.free();
        (function_ptr.cast::<u8>(), Layout::new::<FunctionObj>())
      },
      HeapNativeFn(native_fn_ptr) => {
        unsafe { &mut **native_fn_ptr }.free();
        (native_fn_ptr.cast::<u8>(), Layout::new::<NativeFnObj>())
      },
      HeapObjInstance(obj_instance_ptr) => {
        unsafe { &mut **obj_instance_ptr }.free();
        (obj_instance_ptr.cast::<u8>(), Layout::new::<FunctionObj>())
      },
      HeapString(string_ptr) => {
        unsafe { &mut **string_ptr }.free();
        (string_ptr.cast::<u8>(), Layout::new::<StringObj>())
      },
      HeapUpvalue(upvalue_ptr) => {
        unsafe { &mut **upvalue_ptr }.free();
        (upvalue_ptr.cast::<u8>(), Layout::new::<UpvalueObj>())
      },
    };

    unsafe { dealloc(ptr, layout) };
  }
}

#[derive(PartialEq)]
#[repr(C)]
pub struct BoundMethodObj {
  pub receiver: Value,
  pub method_gc_ptr: *mut GcObject, // *ClosureObj
}

impl BoundMethodObj {
  pub fn method(&self) -> &ClosureObj {
    let HeapClosure(closure_obj_ptr) = self.method_gc().object else {
      panic!("Bound method cannot contain non-closure");
    };
    unsafe { &mut *closure_obj_ptr }
  }

  pub fn method_gc(&self) -> &GcObject {
    unsafe { &*self.method_gc_ptr }
  }

  pub fn method_gc_mut(&mut self) -> &mut GcObject {
    unsafe { &mut *self.method_gc_ptr }
  }
}

impl Display for BoundMethodObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "{}", self.method().function())
  }
}

impl Freeable for BoundMethodObj {
  fn free(&mut self) {} // It doesn't own anything that it references. --Jason B. (8/29/26)
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct ClassObj {
  pub name_gc_ptr: *mut GcObject, // *StringObj
  pub methods: HashTable,
}

impl ClassObj {
  pub const fn new(name_gc_ptr: *mut GcObject) -> Self {
    Self { name_gc_ptr, methods: HashTable::new() }
  }

  pub fn name(&self) -> &StringObj {
    let HeapString(name_obj_ptr) = self.name_gc().object else {
      panic!("Class name cannot be non-string");
    };
    unsafe { &mut *name_obj_ptr }
  }

  pub fn name_gc(&self) -> &GcObject {
    unsafe { &*self.name_gc_ptr }
  }

  pub fn name_gc_mut(&mut self) -> &mut GcObject {
    unsafe { &mut *self.name_gc_ptr }
  }
}

impl Display for ClassObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "{}", self.name())
  }
}

impl Freeable for ClassObj {
  fn free(&mut self) {
    self.methods.free();
  }
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct ClosureObj {
  pub function_gc_ptr: *mut GcObject,       // *FunctionObj
  pub upvalues_ptr_ptr: *mut *mut GcObject, // **UpvalueObj
  pub upvalue_count: u16,
}

impl ClosureObj {
  pub fn function(&self) -> &FunctionObj {
    let HeapFunction(fn_ptr) = unsafe { &*self.function_gc_ptr }.object else {
      panic!("Illegal for closure's function pointer to be to a non-function");
    };

    unsafe { &*fn_ptr }
  }
}

impl Display for ClosureObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    self.function().fmt(formatter)
  }
}

impl Freeable for ClosureObj {
  fn free(&mut self) {
    let layout = Layout::array::<*mut UpvalueObj>(self.upvalue_count as usize).unwrap();
    unsafe {
      dealloc(self.upvalues_ptr_ptr.cast(), layout);
    }
  }
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub enum FunctionObj {
  MainScript { arity: u32, chunk: Chunk, upvalue_count: u16 },
  UserDefined { arity: u32, chunk: Chunk, name_gc_ptr: *mut GcObject, upvalue_count: u16 },
}
use FunctionObj::{MainScript, UserDefined};

impl FunctionObj {
  pub const fn arity(&self) -> u32 {
    match self {
      MainScript { arity, .. } | UserDefined { arity, .. } => *arity,
    }
  }

  pub const fn chunk(&self) -> &Chunk {
    match self {
      MainScript { chunk, .. } | UserDefined { chunk, .. } => chunk,
    }
  }

  pub const fn chunk_mut(&mut self) -> &mut Chunk {
    match self {
      MainScript { chunk, .. } | UserDefined { chunk, .. } => chunk,
    }
  }

  pub const fn increment_upvalue_count(&mut self) {
    match self {
      MainScript { upvalue_count, .. } | UserDefined { upvalue_count, .. } => {
        *upvalue_count += 1;
      },
    }
  }

  pub const fn upvalue_count(&self) -> u16 {
    match self {
      MainScript { upvalue_count, .. } | UserDefined { upvalue_count, .. } => *upvalue_count,
    }
  }

  pub const fn set_arity(&mut self, new_arity: u32) {
    match self {
      MainScript { arity, .. } | UserDefined { arity, .. } => {
        *arity = new_arity;
      },
    }
  }
}

impl Display for FunctionObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    match self {
      MainScript { .. } => write!(formatter, "<script>"),
      UserDefined { name_gc_ptr, .. } => {
        let HeapString(name_ptr) = unsafe { &**name_gc_ptr }.object else {
          panic!("Not possible for name pointer to be non-string");
        };
        write!(formatter, "<fn {}>", unsafe { &*name_ptr }.to_text())
      },
    }
  }
}

impl Freeable for FunctionObj {
  fn free(&mut self) {
    let _ = match self {
      MainScript { chunk, .. } | UserDefined { chunk, .. } => chunk.free(),
    };
  }
}

#[repr(C)]
pub struct NativeFnObj(pub Box<dyn Fn(u8, *mut Value) -> Value>);

impl NativeFnObj {
  pub fn invoke(&self, arg_count: u8, args_ptr: *mut Value) -> Value {
    (*self.0)(arg_count, args_ptr)
  }
}

impl Display for NativeFnObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "<native fn>")
  }
}

impl Eq for NativeFnObj {}

impl Freeable for NativeFnObj {
  fn free(&mut self) {}
}

impl PartialEq for NativeFnObj {
  fn eq(&self, _other: &Self) -> bool {
    false
  }
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct ObjInstanceObj {
  class_gc_ptr: *mut GcObject,
  pub fields: HashTable,
}

impl ObjInstanceObj {
  pub const fn new(class_gc_ptr: *mut GcObject) -> Self {
    Self { class_gc_ptr, fields: HashTable::new() }
  }

  pub(super) fn class_gc_mut(&mut self) -> &mut GcObject {
    unsafe { &mut *self.class_gc_ptr }
  }

  pub fn class(&self) -> &ClassObj {
    unsafe { &*self.class_obj_ptr() }
  }

  fn class_mut(&mut self) -> &mut ClassObj {
    unsafe { &mut *self.class_obj_ptr() }
  }

  fn class_obj_ptr(&self) -> *mut ClassObj {
    let HeapClass(class_obj_ptr) = unsafe { &*self.class_gc_ptr }.object else {
      panic!("Instance's class must be a class");
    };
    class_obj_ptr
  }
}

impl Display for ObjInstanceObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "{} instance", self.class().name())
  }
}

impl Freeable for ObjInstanceObj {
  fn free(&mut self) {
    self.fields.free();
    self.class_mut().free();
  }
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct StringObj {
  pub chars: *const u8,
  pub length: usize,
  pub hash: u32,
}

impl StringObj {
  pub fn to_text(&self) -> String {
    let bytes = unsafe { from_raw_parts(self.chars, self.length) };
    String::from_utf8(bytes.to_vec()).expect("Invalid UTF-8 bytes")
  }
}

impl Display for StringObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "{}", self.to_text())
  }
}

impl Freeable for StringObj {
  fn free(&mut self) {
    let layout = Layout::array::<u8>(self.length).unwrap();
    unsafe { dealloc(self.chars.cast_mut(), layout) };
  }
}

#[derive(PartialEq)]
#[repr(C)]
pub struct UpvalueObj {
  pub closed_value: Value,
  pub next_gc_opt: Option<*mut GcObject>,
  pub value_ptr: *mut Value,
}

impl Display for UpvalueObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    write!(formatter, "upvalue")
  }
}

impl Freeable for UpvalueObj {
  fn free(&mut self) {}
}

pub fn objs_are_equal(a: &HeapObject, b: &HeapObject) -> bool {
  match (&a, &b) {
    (HeapString(ptr1), HeapString(ptr2)) => ptr1 == ptr2,
    (HeapClass(ptr1), HeapClass(ptr2)) => ptr1 == ptr2,
    (HeapBoundMethod(ptr1), HeapBoundMethod(ptr2)) => ptr1 == ptr2,
    (_, _) => false,
  }
}

impl Heap {
  pub fn close_upvalues(&mut self, prev_value_ptr: *mut Value) {
    while let Some(upvalue_gc_ptr) = self.head_open_upvalue_gc_opt
      && let HeapUpvalue(upvalue_ptr) = unsafe { &*upvalue_gc_ptr }.object
      && let upvalue = unsafe { &mut *upvalue_ptr }
      && upvalue.value_ptr >= prev_value_ptr
    {
      unsafe {
        upvalue.closed_value = (*upvalue.value_ptr).clone();
        upvalue.value_ptr = &raw mut upvalue.closed_value;
      }
      self.head_open_upvalue_gc_opt = upvalue.next_gc_opt;
    }
  }
}
