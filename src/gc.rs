use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::fmt::{Display, Formatter, Result};
use std::ptr::{copy_nonoverlapping, null_mut};
use std::slice::from_raw_parts;

use crate::hash_table::HashTable;

use crate::memory::free_array;

use crate::value::Value::Nil;

#[repr(C)]
pub struct GcObject {
  pub next: *mut Self,
  pub object: HeapObject,
}

impl GcObject {
  pub fn free(&self) {
    match &self.object {
      HeapObject::HeapString(string_ptr) => {
        let string = unsafe { &**string_ptr };
        let layout = Layout::array::<u8>(string.length).unwrap();
        unsafe { dealloc(string.chars.cast_mut(), layout) };
      },
    }
  }
}

#[derive(Clone)]
pub struct Reference(pub HeapObject);

#[derive(Clone, Eq, PartialEq)]
pub enum HeapObject {
  HeapString(*const StringObj),
}
use HeapObject::HeapString;

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

pub struct Gc {
  pub(super) globals: HashTable,
  objects: *mut GcObject,
  strings: HashTable,
}

impl Gc {
  pub const fn new() -> Self {
    Self { globals: HashTable::new(), objects: null_mut(), strings: HashTable::new() }
  }

  fn allocate_object(&mut self, object: HeapObject) {
    let gc_object = Box::new(GcObject { next: self.objects, object });
    let ptr = Box::into_raw(gc_object);
    self.objects = ptr;
  }

  fn allocate_string(&mut self, chars: *const u8, length: usize, hash: u32) -> HeapObject {
    let string_obj = StringObj { chars, length, hash };
    let string_ptr = Box::into_raw(Box::new(string_obj));

    let heap_obj = HeapObject::HeapString(string_ptr);
    self.allocate_object(heap_obj.clone());

    self.strings.set(string_ptr, Nil);

    heap_obj
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
      Reference(HeapString(interned))
    } else {
      Reference(self.allocate_string(ptr, length, hash))
    }
    // End `take_string`
  }

  pub fn copy_string(&mut self, str: &str, start_index: usize, length: usize) -> Reference {
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
      Reference(HeapString(interned))
    } else {
      let object = self.allocate_string(ptr, length, hash);
      Reference(object)
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
