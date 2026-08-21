use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::fmt::{Display, Formatter, Result};
use std::ptr::{copy_nonoverlapping, null_mut};
use std::slice::from_raw_parts;

#[repr(C)]
pub struct GcObject {
  pub next: *mut Self,
  pub object: HeapObject,
}

impl GcObject {
  pub fn free(&self) {
    match &self.object {
      HeapObject::HeapString(string) => {
        let layout = Layout::array::<u8>(string.length as usize).unwrap();
        unsafe { dealloc(string.chars.cast_mut(), layout) };
      },
    }
  }
}

pub struct Reference(pub *const GcObject);

pub enum HeapObject {
  HeapString(StringObj),
}
use HeapObject::HeapString;

#[repr(C)]
pub struct StringObj {
  pub chars: *const u8,
  pub length: u32,
}

impl Display for StringObj {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    let bytes = unsafe { from_raw_parts(self.chars, self.length as usize) };
    write!(formatter, "\"{}\"", String::from_utf8(bytes.to_vec()).expect("Invalid UTF-8 bytes"))
  }
}

pub struct Gc {
  objects: *mut GcObject,
}

impl Gc {
  pub const fn new() -> Self {
    Self { objects: null_mut() }
  }

  fn allocate_object(&mut self, reference: HeapObject) -> *mut GcObject {
    let object = Box::new(GcObject { next: self.objects, object: reference });
    let ptr = Box::into_raw(object);
    self.objects = ptr;
    ptr
  }

  fn allocate_string(&mut self, chars: *const u8, length: u32) -> *const GcObject {
    self.allocate_object(HeapObject::HeapString(StringObj { chars, length }))
  }

  pub fn concatenate_strings(&mut self, str1: &StringObj, str2: &StringObj) -> Reference {
    let length = str1.length + str2.length;

    let layout = Layout::array::<u8>(length as usize).unwrap();
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
      handle_alloc_error(layout);
    }

    unsafe {
      copy_nonoverlapping(str1.chars, ptr, str1.length as usize);
      copy_nonoverlapping(str2.chars, ptr.add(str1.length as usize), str2.length as usize);
    }

    Reference(self.allocate_string(ptr, length))
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

    let object = self.allocate_string(ptr, u32::try_from(length).unwrap());
    Reference(object)
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
  }
}

fn _free_chars(chars: *const u8, length: u32) {
  let layout = Layout::array::<u8>(length as usize).unwrap();
  unsafe {
    dealloc(chars.cast_mut(), layout);
  }
}

pub fn refs_are_equal(a: &Reference, b: &Reference) -> bool {
  let ax = unsafe { &*a.0 };
  let bx = unsafe { &*b.0 };
  heap_objects_are_equal(&ax.object, &bx.object)
}

pub fn heap_objects_are_equal(a: &HeapObject, b: &HeapObject) -> bool {
  match (a, b) {
    (
      HeapString(StringObj { chars: chars1, length: length1 }),
      HeapString(StringObj { chars: chars2, length: length2 }),
    ) => {
      if length1 == length2 {
        let a = unsafe { from_raw_parts(*chars1, *length1 as usize) };
        let b = unsafe { from_raw_parts(*chars2, *length2 as usize) };
        a == b
      } else {
        false
      }
    },
  }
}
