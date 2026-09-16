use std::alloc::{self, Layout, handle_alloc_error};
use std::mem::{align_of, size_of};
use std::ops::Mul;
use std::ptr::null_mut;

#[inline]
#[allow(clippy::missing_panics_doc, clippy::missing_safety_doc)]
pub unsafe fn free_array<T, U: Into<usize>>(pointer: *mut T, old_count: U) -> *mut T {
  let old_count = old_count.into();
  if !pointer.is_null() {
    let layout = Layout::from_size_align(size_of::<T>() * old_count, align_of::<T>()).unwrap();
    unsafe {
      alloc::dealloc(pointer.cast::<u8>(), layout);
    }
  }
  null_mut()
}

#[inline]
#[allow(clippy::missing_panics_doc, clippy::missing_safety_doc)]
pub unsafe fn grow_array<T, U: Into<usize>>(pointer: *mut T, old_count: U, new_count: U) -> *mut T {
  let old_count = old_count.into();
  let new_count = new_count.into();

  let new_size = size_of::<T>() * new_count;
  if pointer.is_null() {
    // Allocate new
    let layout = Layout::from_size_align(new_size, align_of::<T>()).unwrap();
    unsafe { alloc::alloc(layout).cast::<T>() }
  } else {
    // Resize
    let old_layout = Layout::from_size_align(size_of::<T>() * old_count, align_of::<T>()).unwrap();
    let result = unsafe { alloc::realloc(pointer.cast::<u8>(), old_layout, new_size).cast::<T>() };
    if result.is_null() {
      eprintln!("Reallocation failed");
      handle_alloc_error(old_layout);
    } else {
      result
    }
  }
}

#[inline]
pub fn next_capacity<T>(old_capacity: T) -> T
where
  T: PartialOrd + Mul<Output = T> + From<u8>,
{
  let eight = T::from(8);

  if old_capacity < eight {
    eight
  } else {
    old_capacity * T::from(2)
  }
}

pub trait Freeable {
  fn free(&mut self);
}
