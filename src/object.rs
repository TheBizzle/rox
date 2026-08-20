use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::fmt::{Display, Formatter, Result};
use std::ptr::copy_nonoverlapping;
use std::rc::Rc;
use std::slice::from_raw_parts;
use std::sync::Mutex;

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

impl StringObj {
  pub fn concatenate(&self, str: &Self, objects: &Rc<Mutex<*mut GcObject>>) -> Reference {
    let length = self.length + str.length;

    let layout = Layout::array::<u8>(length as usize).unwrap();
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
      handle_alloc_error(layout);
    }

    unsafe {
      copy_nonoverlapping(self.chars, ptr, self.length as usize);
      copy_nonoverlapping(str.chars, ptr.add(self.length as usize), str.length as usize);
    }

    Reference(allocate_string(ptr, length, objects))
  }
}

fn allocate_object(reference: HeapObject, objects: &Rc<Mutex<*mut GcObject>>) -> *mut GcObject {
  let object = Box::new(GcObject { next: *(*objects).lock().unwrap(), object: reference });
  let ptr = Box::into_raw(object);
  *objects.lock().unwrap() = ptr;
  ptr
}

fn allocate_string(chars: *const u8, length: u32, objects: &Rc<Mutex<*mut GcObject>>) -> *const GcObject {
  allocate_object(HeapObject::HeapString(StringObj { chars, length }), objects)
}

pub fn copy_string(
  str: &str, start_index: usize, length: usize, objects: &Rc<Mutex<*mut GcObject>>,
) -> Reference {
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

  let object = allocate_string(ptr, u32::try_from(length).unwrap(), objects);
  Reference(object)
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
