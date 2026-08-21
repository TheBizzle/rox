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
      HeapObject::HeapString(string_ptr) => {
        let string = unsafe { &**string_ptr };
        let layout = Layout::array::<u8>(string.length as usize).unwrap();
        unsafe { dealloc(string.chars.cast_mut(), layout) };
      },
    }
  }
}

pub struct Reference(pub HeapObject);

#[derive(Clone)]
pub enum HeapObject {
  HeapString(*const StringObj),
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

  fn allocate_object(&mut self, object: HeapObject) {
    let gc_object = Box::new(GcObject { next: self.objects, object });
    let ptr = Box::into_raw(gc_object);
    self.objects = ptr;
  }

  fn allocate_string(&mut self, chars: *const u8, len: usize) -> HeapObject {
    let length = u32::try_from(len).unwrap();
    let string_obj = StringObj { chars, length };
    let string_ptr = Box::into_raw(Box::new(string_obj));

    let heap_obj = HeapObject::HeapString(string_ptr);
    self.allocate_object(heap_obj.clone());

    heap_obj
  }

  pub fn concatenate_strings(&mut self, string1: *const StringObj, string2: *const StringObj) -> Reference {
    let str1 = unsafe { &*string1 };
    let str2 = unsafe { &*string2 };

    let length1 = str1.length as usize;
    let length2 = str2.length as usize;
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

    let object = self.allocate_string(ptr, length);
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
  match (&a.0, &b.0) {
    (HeapString(ptr1), HeapString(ptr2)) => {
      let (StringObj { chars: chars1, length: length1 }, StringObj { chars: chars2, length: length2 }) =
        (unsafe { &**ptr1 }, unsafe { &**ptr2 });
      if length1 == length2 {
        let a = unsafe { from_raw_parts(chars1, *length1 as usize) };
        let b = unsafe { from_raw_parts(chars2, *length2 as usize) };
        a == b
      } else {
        false
      }
    },
  }
}
