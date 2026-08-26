use std::alloc::{self, Layout, handle_alloc_error};
use std::mem::{align_of, size_of};

macro_rules! free_array {
  ($type: ty, $pointer: expr, $old_count: expr) => {
    $crate::memory::reallocate::<$type>($pointer, $old_count as usize, 0)
  };
}

macro_rules! grow_array {
  ($type: ty, $pointer: expr, $old_count: expr, $new_count: expr) => {
    $crate::memory::reallocate::<$type>($pointer, $old_count as usize, $new_count as usize)
  };
}

macro_rules! next_capacity {
  ($old_capacity: expr) => {
    if $old_capacity < 8 {
      8
    } else {
      $old_capacity * 2
    }
  };
}

pub(super) use free_array;
pub(super) use grow_array;
pub(super) use next_capacity;

pub trait Freeable {
  fn free(&mut self);
}

pub unsafe fn reallocate<T>(pointer: *mut T, old_count: usize, new_count: usize) -> *mut T {
  if new_count == 0 {
    if !pointer.is_null() {
      // Free memory
      let layout = Layout::from_size_align(size_of::<T>() * old_count, align_of::<T>()).unwrap();
      unsafe {
        alloc::dealloc(pointer.cast::<u8>(), layout);
      }
    }
    std::ptr::null_mut()
  } else if pointer.is_null() {
    // Allocate new
    let new_size = size_of::<T>() * new_count;
    let layout = Layout::from_size_align(new_size, align_of::<T>()).unwrap();
    unsafe { alloc::alloc(layout).cast::<T>() }
  } else {
    // Resize
    let new_size = size_of::<T>() * new_count;
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
