use std::alloc::{Layout, alloc, alloc_zeroed, dealloc, handle_alloc_error};
use std::fmt::{Display, Formatter, Result};
use std::ptr::{copy_nonoverlapping, null_mut};
use std::slice::from_raw_parts;

use crate::chunk::Chunk;

use crate::hash_table::HashTable;

use crate::memory::{Freeable, free_array};

use crate::value::Value::{self, Nil};

#[repr(C)]
pub struct GcObject {
  pub next: *mut Self,
  pub object: HeapObject,
}

impl Freeable for GcObject {
  fn free(&mut self) {
    self.object.free();
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference(pub HeapObject);

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HeapObject {
  HeapClosure(*mut ClosureObj),
  HeapFunction(*mut FunctionObj),
  HeapNativeFn(*mut NativeFnObj),
  HeapString(*mut StringObj),
  HeapUpvalue(*mut UpvalueObj),
}
use HeapObject::{HeapClosure, HeapFunction, HeapNativeFn, HeapString, HeapUpvalue};

impl Freeable for HeapObject {
  fn free(&mut self) {
    match &self {
      HeapClosure(function_obj_ptr) => {
        unsafe { &mut **function_obj_ptr }.free();
        let layout = Layout::new::<ClosureObj>();
        unsafe { dealloc(function_obj_ptr.cast::<u8>(), layout) };
      },
      HeapFunction(function_ptr) => {
        unsafe { &mut **function_ptr }.free();
        let layout = Layout::new::<FunctionObj>();
        unsafe { dealloc(function_ptr.cast::<u8>(), layout) };
      },
      HeapNativeFn(native_fn_ptr) => {
        unsafe { &mut **native_fn_ptr }.free();
        let layout = Layout::new::<NativeFnObj>();
        unsafe { dealloc(native_fn_ptr.cast::<u8>(), layout) };
      },
      HeapString(string_ptr) => {
        unsafe { &mut **string_ptr }.free();
        let layout = Layout::new::<StringObj>();
        unsafe { dealloc(string_ptr.cast::<u8>(), layout) };
      },
      HeapUpvalue(upvalue_ptr) => {
        unsafe { &mut **upvalue_ptr }.free();
        let layout = Layout::new::<UpvalueObj>();
        unsafe { dealloc(upvalue_ptr.cast::<u8>(), layout) };
      },
    }
  }
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct ClosureObj {
  pub function_obj_ptr: *mut FunctionObj,
  pub upvalues_ptr_ptr: *mut *mut UpvalueObj,
  pub upvalue_count: u8,
}

impl Display for ClosureObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    unsafe { &*self.function_obj_ptr }.fmt(formatter)
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
  MainScript { arity: u32, chunk: Chunk, upvalue_count: u8 },
  UserDefined { arity: u32, chunk: Chunk, name_ptr: *const StringObj, upvalue_count: u8 },
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

  pub const fn upvalue_count(&self) -> u8 {
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
      UserDefined { name_ptr, .. } => {
        let fn_name = unsafe { &**name_ptr }.chars;
        write!(formatter, "<fn {fn_name:?}>")
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
pub struct StringObj {
  pub chars: *const u8,
  pub length: usize,
  pub hash: u32,
}

impl Display for StringObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    let bytes = unsafe { from_raw_parts(self.chars, self.length) };
    write!(formatter, "\"{}\"", String::from_utf8(bytes.to_vec()).expect("Invalid UTF-8 bytes"))
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
  pub next_opt: Option<*mut Self>,
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

pub struct Gc {
  pub(super) globals: HashTable,
  pub(super) head_open_upvalue_opt: Option<*mut UpvalueObj>,
  objects: *mut GcObject,
  strings: HashTable,
}

impl Gc {
  pub const fn new() -> Self {
    Self {
      head_open_upvalue_opt: None,
      globals: HashTable::new(),
      objects: null_mut(),
      strings: HashTable::new(),
    }
  }

  #[allow(clippy::cast_ptr_alignment)]
  pub fn allocate_closure(&mut self, function_obj_ptr: *mut FunctionObj) -> *mut ClosureObj {
    let function_obj = unsafe { &*function_obj_ptr };
    let upvalue_count = function_obj.upvalue_count();
    let layout = Layout::array::<*mut UpvalueObj>(upvalue_count as usize).unwrap();
    let upvalues_ptr_ptr = unsafe { alloc_zeroed(layout).cast::<*mut UpvalueObj>() };

    let closure_obj = ClosureObj { function_obj_ptr, upvalues_ptr_ptr, upvalue_count };
    self.allocate_on_heap(closure_obj, HeapClosure)
  }

  pub fn allocate_function(&mut self, function_obj: FunctionObj) -> *mut FunctionObj {
    self.allocate_on_heap(function_obj, HeapFunction)
  }

  pub fn allocate_native_fn(&mut self, native_fn_obj: NativeFnObj) -> *mut NativeFnObj {
    self.allocate_on_heap(native_fn_obj, HeapNativeFn)
  }

  fn allocate_string(&mut self, chars: *const u8, length: usize, hash: u32) -> *mut StringObj {
    let string_obj = StringObj { chars, length, hash };
    let string_ptr = self.allocate_on_heap(string_obj, HeapString);
    self.strings.set(string_ptr, Nil);
    string_ptr
  }

  pub fn allocate_upvalue(&mut self, upvalue_obj: UpvalueObj) -> *mut UpvalueObj {
    self.allocate_on_heap(upvalue_obj, HeapUpvalue)
  }

  fn allocate_on_heap<T, F: Fn(*mut T) -> HeapObject>(&mut self, obj: T, constructor: F) -> *mut T {
    let ptr = Box::into_raw(Box::new(obj));
    let object = constructor(ptr);
    let gc_object = Box::new(GcObject { next: self.objects, object });
    let gc_ptr = Box::into_raw(gc_object);
    self.objects = gc_ptr;
    ptr
  }

  pub fn close_upvalues(&mut self, prev_value_ptr: *mut Value) {
    while let Some(upvalue_ptr) = self.head_open_upvalue_opt
      && let upvalue = unsafe { &mut *upvalue_ptr }
      && upvalue.value_ptr >= prev_value_ptr
    {
      unsafe {
        upvalue.closed_value = (*upvalue.value_ptr).clone();
        upvalue.value_ptr = &raw mut upvalue.closed_value;
      }
      self.head_open_upvalue_opt = upvalue.next_opt;
    }
  }

  pub fn concatenate_strings(&mut self, string1: *const StringObj, string2: *const StringObj) -> Reference {
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

    // TODO: Start `take_string`
    let hash = hash_string(ptr, length);
    #[allow(clippy::option_if_let_else)]
    if let Some(interned) = self.strings.find_string(ptr, length, hash) {
      unsafe {
        free_array!(u8, ptr, length);
      }
      Reference(HeapString(interned.cast_mut()))
    } else {
      Reference(HeapString(self.allocate_string(ptr, length, hash)))
    }
    // End `take_string`
  }

  pub fn copy_string(&mut self, str: &str, start_index: usize, length: usize) -> *mut StringObj {
    let layout = Layout::array::<u8>(length).unwrap();
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
      eprintln!("Reallocation for characters failed");
      handle_alloc_error(layout);
    }

    let substring = &str[start_index..(start_index + length)];
    unsafe {
      copy_nonoverlapping(substring.as_ptr(), ptr, length);
    }

    let hash = hash_string(ptr, length);
    #[allow(clippy::option_if_let_else)]
    if let Some(interned) = self.strings.find_string(ptr, length, hash) {
      interned.cast_mut()
    } else {
      self.allocate_string(ptr, length, hash)
    }
  }

  pub fn free(&mut self) {
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
    self.strings.free();
  }
}

fn _free_chars(chars: *const u8, length: u32) {
  let layout = Layout::array::<u8>(length as usize).unwrap();
  unsafe {
    dealloc(chars.cast_mut(), layout);
  }
}

pub fn refs_are_equal(a: &Reference, b: &Reference) -> bool {
  match (&a.0, &b.0) {
    (HeapString(ptr1), HeapString(ptr2)) => ptr1 == ptr2,
    (_, _) => false,
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
