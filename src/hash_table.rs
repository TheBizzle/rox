use std::alloc::{Layout, alloc, handle_alloc_error};
use std::ptr::{addr_of, null_mut};
use std::slice::from_raw_parts;

use crate::gc::StringObj;

use crate::memory::{free_array, next_capacity};

use crate::value::Value;

const TABLE_MAX_LOAD: f64 = 0.75;

#[repr(C)]
pub struct HashTable {
  capacity: usize,
  count: usize,
  cells_ptr: *mut Cell,
}

#[repr(C)]
enum Cell {
  NeverFilled,
  Tombstone,
  Entry { key: *const StringObj, value: Value },
}
use Cell::{Entry, NeverFilled, Tombstone};

impl HashTable {
  pub const fn new() -> Self {
    Self { count: 0, capacity: 0, cells_ptr: null_mut() }
  }

  #[allow(unused)]
  fn add_all(&mut self, target: &mut Self) {
    for i in 0..self.capacity {
      if let Entry { key, value } = unsafe { &*self.cells_ptr.add(i) } {
        target.set(*key, value.clone());
      }
    }
  }

  fn adjust_capacity(&mut self, capacity: usize) {
    let layout = Layout::array::<Cell>(capacity).unwrap();

    #[allow(clippy::cast_ptr_alignment)]
    let new_cells_ptr = unsafe { alloc(layout) }.cast::<Cell>();

    if new_cells_ptr.is_null() {
      handle_alloc_error(layout);
    }

    for i in 0..capacity {
      unsafe {
        new_cells_ptr.add(i).write(NeverFilled);
      }
    }

    self.count = 0;
    for i in 0..self.capacity {
      let ptr = unsafe { self.cells_ptr.add(i) };
      if let Entry { key, .. } = unsafe { &*ptr } {
        let cell_ptr = find_cell(new_cells_ptr, capacity, *key);
        unsafe {
          cell_ptr.write(ptr.read());
        }
        self.count += 1;
      }
    }

    unsafe {
      free_array!(Cell, self.cells_ptr, self.capacity);
    }

    self.cells_ptr = new_cells_ptr;
    self.capacity = capacity;
  }

  #[allow(clippy::needless_pass_by_ref_mut)]
  #[allow(unused)]
  fn delete(&mut self, key: *const StringObj) -> bool {
    if self.count == 0 {
      false
    } else {
      let ptr = self.find_cell(key);
      if matches!(unsafe { &*ptr }, Entry { .. }) {
        unsafe {
          ptr.write(Tombstone);
        }
        true
      } else {
        false
      }
    }
  }

  #[allow(clippy::option_option)]
  fn find_cell(&self, key_ptr: *const StringObj) -> *mut Cell {
    find_cell(self.cells_ptr, self.capacity, key_ptr)
  }

  pub fn find_string(&self, chars: *const u8, length: usize, hash: u32) -> Option<*const StringObj> {
    if self.count == 0 {
      None
    } else {
      let mut index = (hash as usize) % self.capacity;
      loop {
        match unsafe { &*self.cells_ptr.add(index) } {
          NeverFilled => {
            return None;
          },
          Entry { key: key_ptr, .. }
            if {
              let key = unsafe { &**key_ptr };
              key.length == length && key.hash == hash && {
                let a = unsafe { from_raw_parts(key.chars, key.length) };
                let b = unsafe { from_raw_parts(chars, length) };
                a == b
              }
            } =>
          {
            return Some(*key_ptr);
          },
          _ => {
            index = (index + 1) % self.capacity;
          },
        }
      }
    }
  }

  pub fn free(&mut self) {
    unsafe {
      free_array!(Cell, self.cells_ptr, self.capacity);
    }

    self.count = 0;
    self.capacity = 0;
    self.cells_ptr = null_mut();
  }

  #[allow(unused)]
  pub fn get(&self, key: *const StringObj) -> Option<*const Value> {
    if self.count == 0 {
      None
    } else {
      let ptr = self.find_cell(key);
      match unsafe { &*ptr } {
        NeverFilled | Tombstone => None,
        Entry { value, .. } => Some(unsafe { addr_of!(*value) }),
      }
    }
  }

  pub fn set(&mut self, key: *const StringObj, value: Value) -> bool {
    #[allow(clippy::cast_precision_loss)]
    if ((self.count + 1) as f64) > ((self.capacity as f64) * TABLE_MAX_LOAD) {
      self.adjust_capacity(next_capacity!(self.capacity));
    }

    let ptr = self.find_cell(key);

    let is_new = match unsafe { &*ptr } {
      Entry { .. } => false,
      Tombstone => true,
      NeverFilled => {
        self.count += 1;
        true
      },
    };

    unsafe {
      ptr.write(Entry { key, value });
    }

    is_new
  }
}

#[allow(clippy::option_option)]
fn find_cell(entry_opts: *mut Cell, capacity: usize, key_ptr: *const StringObj) -> *mut Cell {
  let key = unsafe { &*key_ptr };
  let mut index = (key.hash as usize) % capacity;

  let mut last_tombstone_ptr_opt: Option<*mut Cell> = None;

  loop {
    let ptr = unsafe { entry_opts.add(index) };
    let cell = unsafe { &*ptr };

    match cell {
      Entry { key, .. } if key == &key_ptr => {
        return ptr;
      },
      Entry { .. } => {
        index = (index + 1) % capacity;
      },
      Tombstone => {
        last_tombstone_ptr_opt = Some(ptr);
        index = (index + 1) % capacity;
      },
      NeverFilled => {
        return last_tombstone_ptr_opt.unwrap_or(ptr);
      },
    }
  }
}
