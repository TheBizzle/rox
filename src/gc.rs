use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
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

#[derive(Clone, Debug)]
pub struct Reference(pub HeapObject);

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HeapObject {
  HeapFunction(*mut FunctionObj),
  HeapNativeFn(*mut NativeFnObj),
  HeapString(*mut StringObj),
}
use HeapObject::{HeapFunction, HeapNativeFn, HeapString};

impl Freeable for HeapObject {
  fn free(&mut self) {
    match &self {
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
    }
  }
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub enum FunctionObj {
  MainScript { arity: u32, chunk: Chunk },
  UserDefined { arity: u32, chunk: Chunk, name_ptr: *const StringObj },
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

pub struct Gc {
  pub(super) globals: HashTable,
  objects: *mut GcObject,
  strings: HashTable,
}

impl Gc {
  pub const fn new() -> Self {
    Self { globals: HashTable::new(), objects: null_mut(), strings: HashTable::new() }
  }

  pub fn allocate_function(&mut self, function_obj: FunctionObj) -> *mut FunctionObj {
    let function_ptr = Box::into_raw(Box::new(function_obj));

    let heap_obj = HeapFunction(function_ptr);
    self.allocate_object(heap_obj);

    function_ptr
  }

  pub fn allocate_native_fn(&mut self, native_fn_obj: NativeFnObj) -> *mut NativeFnObj {
    let native_fn_ptr = Box::into_raw(Box::new(native_fn_obj));

    let heap_obj = HeapNativeFn(native_fn_ptr);
    self.allocate_object(heap_obj);

    native_fn_ptr
  }

  fn allocate_object(&mut self, object: HeapObject) {
    let gc_object = Box::new(GcObject { next: self.objects, object });
    let ptr = Box::into_raw(gc_object);
    self.objects = ptr;
  }

  fn allocate_string(&mut self, chars: *const u8, length: usize, hash: u32) -> *mut StringObj {
    let string_obj = StringObj { chars, length, hash };
    let string_ptr = Box::into_raw(Box::new(string_obj));

    let heap_obj = HeapString(string_ptr);
    self.allocate_object(heap_obj);

    self.strings.set(string_ptr, Nil);

    string_ptr
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
