use std::alloc::{Layout, alloc, alloc_zeroed, dealloc, handle_alloc_error};
use std::any::type_name;
use std::fmt::{Debug, Display, Formatter, Result};
use std::ptr::{self, copy_nonoverlapping, null_mut};
use std::slice::{from_raw_parts, from_raw_parts_mut};

use crate::chunk::Chunk;

use crate::hash_table::HashTable;

use crate::memory::{Freeable, free_array};

use crate::value::Value::{self, Nil, Reference};
use crate::value::ValueArray;

pub const DEBUG_STRESS_GC: bool = false;
pub const DEBUG_LOG_GC: bool = true;

#[derive(Debug)]
#[repr(C)]
pub struct GcObject {
  pub next: *mut Self,
  pub object: HeapObject,
  is_marked: bool,
}

impl GcObject {
  fn blackenables(&self) -> Vec<Blackenable> {
    if DEBUG_LOG_GC {
      println!("{self:?} blacken {:?}", self.object);
    }

    match self.object {
      HeapBoundMethod(obj_ptr) => {
        let receiver = &(unsafe { &mut *obj_ptr }.receiver);
        let method_ref = unsafe { &mut *obj_ptr }.method_gc_mut();
        vec![Blackenable::Value(receiver), Blackenable::Object(method_ref)]
      },

      HeapClass(obj_ptr) => {
        let name_obj = unsafe { &mut *obj_ptr }.name_gc_mut();
        let methods_ref = &mut unsafe { &mut *obj_ptr }.methods;
        vec![Blackenable::Object(name_obj), Blackenable::Table(methods_ref)]
      },

      HeapClosure(obj_ptr) => {
        let closure = unsafe { &*obj_ptr };
        let function_gc = unsafe { &mut *closure.function_gc_ptr };

        let mut out = Vec::with_capacity(1 + closure.upvalue_count as usize);
        out.push(Blackenable::Object(&mut *function_gc));

        let uvps = unsafe { from_raw_parts_mut(closure.upvalues_ptr_ptr, closure.upvalue_count as usize) };

        for upvalue_ptr in uvps {
          out.push(Blackenable::Object(unsafe { &mut **upvalue_ptr }));
        }

        out
      },

      HeapFunction(obj_ptr) => {
        let function = unsafe { &*obj_ptr };
        match function {
          MainScript { chunk, .. } => vec![Blackenable::Array(&chunk.constants)],
          UserDefined { chunk, name_gc_ptr, .. } => {
            let gc_obj_ref = unsafe { &mut **name_gc_ptr };
            vec![Blackenable::Array(&chunk.constants), Blackenable::Object(gc_obj_ref)]
          },
        }
      },

      HeapObjInstance(obj_ptr) => {
        let class_gc = unsafe { &mut *obj_ptr }.class_gc_mut();
        let fields_ref = &mut (unsafe { &mut *obj_ptr }.fields);
        vec![Blackenable::Object(class_gc), Blackenable::Table(fields_ref)]
      },

      HeapUpvalue(obj_ptr) => {
        let closed_value_ref = &(unsafe { &*obj_ptr }.closed_value);
        vec![Blackenable::Value(closed_value_ref)]
      },

      HeapNativeFn(_) | HeapString(_) => Vec::new(),
    }
  }

  fn set_marked(&mut self) {
    if DEBUG_LOG_GC {
      println!("Now marked: {self:?}");
    }
    self.is_marked = true;
  }
}

impl Freeable for GcObject {
  fn free(&mut self) {
    if DEBUG_LOG_GC {
      println!("{self:?} free type {}", type_name::<Self>());
    }
    self.object.free();
  }
}

type GcPtr = *mut GcObject;
type StrPtr = *mut StringObj;

enum Blackenable {
  Array(&'static ValueArray),
  Object(&'static mut GcObject),
  Table(&'static mut HashTable),
  Value(&'static Value),
}

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
    let name_heap_obj = &mut unsafe { &mut *self.name_gc_ptr }.object;
    name_heap_obj.free();
    self.methods.free();
  }
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct ClosureObj {
  pub function_gc_ptr: *mut GcObject,       // *FunctionObj
  pub upvalues_ptr_ptr: *mut *mut GcObject, // **UpvalueObj
  pub upvalue_count: u8,
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
  MainScript { arity: u32, chunk: Chunk, upvalue_count: u8 },
  UserDefined { arity: u32, chunk: Chunk, name_gc_ptr: *mut GcObject, upvalue_count: u8 },
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
      UserDefined { name_gc_ptr, .. } => {
        let HeapString(name_ptr) = unsafe { &**name_gc_ptr }.object else {
          panic!("Not possible for name pointer to be non-string");
        };
        let name = unsafe { &*name_ptr };
        let fn_name = name.chars;
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
pub struct ObjInstanceObj {
  class_gc_ptr: *mut GcObject,
  pub fields: HashTable,
}

impl ObjInstanceObj {
  pub const fn new(class_gc_ptr: *mut GcObject) -> Self {
    Self { class_gc_ptr, fields: HashTable::new() }
  }

  fn class_gc_mut(&mut self) -> &mut GcObject {
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
    write!(formatter, "\"{}\"", self.to_text())
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

pub struct Gc {
  pub(super) globals: HashTable,
  grays: Vec<*const GcObject>,
  pub(super) head_open_upvalue_gc_opt: Option<*mut GcObject>,
  objects: *mut GcObject,
  strings: HashTable,

  pub(super) init_str_gc_ptr: *mut GcObject,
  pub(super) super_str_gc_ptr: *mut GcObject,
  pub(super) this_str_gc_ptr: *mut GcObject,
}

impl Freeable for Gc {
  fn free(&mut self) {
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
    self.grays.clear();
    self.strings.free();
  }
}

impl Gc {
  pub fn new() -> Self {
    let mut this = Self {
      globals: HashTable::new(),
      grays: Vec::new(),
      head_open_upvalue_gc_opt: None,
      objects: null_mut(),
      strings: HashTable::new(),
      init_str_gc_ptr: null_mut(),
      this_str_gc_ptr: null_mut(),
      super_str_gc_ptr: null_mut(),
    };

    let init_name = "init";
    this.init_str_gc_ptr = this.copy_string(init_name, 0, init_name.len()).1;

    let this_name = "this";
    this.this_str_gc_ptr = this.copy_string(this_name, 0, this_name.len()).1;

    let super_name = "super";
    this.super_str_gc_ptr = this.copy_string(super_name, 0, super_name.len()).1;

    this
  }

  pub fn allocate_bound_method(&mut self, bound_method_obj: BoundMethodObj) -> GcPtr {
    self.allocate_on_heap(bound_method_obj, HeapBoundMethod).1
  }

  pub fn allocate_class(&mut self, class_obj: ClassObj) -> GcPtr {
    self.allocate_on_heap(class_obj, HeapClass).1
  }

  #[allow(clippy::cast_ptr_alignment)]
  pub fn allocate_closure(&mut self, function_gc_ptr: *mut GcObject) -> (*mut ClosureObj, GcPtr) {
    let HeapFunction(function_obj_ptr) = unsafe { &*function_gc_ptr }.object else {
      panic!("Illegal for closure's function pointer to be to a non-function");
    };
    let function_obj = unsafe { &*function_obj_ptr };
    let upvalue_count = function_obj.upvalue_count();
    let layout = Layout::array::<*mut UpvalueObj>(upvalue_count as usize).unwrap();
    let upvalues_ptr_ptr = unsafe { alloc_zeroed(layout).cast::<*mut GcObject>() };

    let closure_obj = ClosureObj { function_gc_ptr, upvalues_ptr_ptr, upvalue_count };
    self.allocate_on_heap(closure_obj, HeapClosure)
  }

  pub fn allocate_function(&mut self, function_obj: FunctionObj) -> GcPtr {
    self.allocate_on_heap(function_obj, HeapFunction).1
  }

  pub fn allocate_native_fn(&mut self, native_fn_obj: NativeFnObj) -> GcPtr {
    self.allocate_on_heap(native_fn_obj, HeapNativeFn).1
  }

  pub fn allocate_obj_instance(&mut self, obj_instance_obj: ObjInstanceObj) -> GcPtr {
    self.allocate_on_heap(obj_instance_obj, HeapObjInstance).1
  }

  fn allocate_string(&mut self, chars: *const u8, length: usize, hash: u32) -> (StrPtr, GcPtr) {
    let string_obj = StringObj { chars, length, hash };
    let (string_ptr, gc_ptr) = self.allocate_on_heap(string_obj, HeapString);
    self.strings.set(gc_ptr, Nil);
    (string_ptr, gc_ptr)
  }

  pub fn allocate_upvalue(&mut self, upvalue_obj: UpvalueObj) -> (*mut UpvalueObj, GcPtr) {
    self.allocate_on_heap(upvalue_obj, HeapUpvalue)
  }

  fn allocate_on_heap<T, F: Fn(*mut T) -> HeapObject>(&mut self, obj: T, constructor: F) -> (*mut T, GcPtr) {
    let ptr = Box::into_raw(Box::new(obj));

    if DEBUG_LOG_GC {
      let object = constructor(ptr);
      let gc_object = GcObject { next: self.objects, object: object.clone(), is_marked: false };
      println!("{object:?} allocate {} for {:?}", std::mem::size_of_val(&gc_object), type_name::<T>());
    }

    let object = constructor(ptr);
    let gc_object = GcObject { next: self.objects, object, is_marked: false };
    let gc_ptr = Box::into_raw(Box::new(gc_object));
    self.objects = gc_ptr;
    (ptr, gc_ptr)
  }

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

  pub fn concatenate_strings(&mut self, string1: StrPtr, string2: StrPtr) -> (StrPtr, GcPtr) {
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

    let hash = hash_string(ptr, length);
    #[allow(clippy::option_if_let_else)]
    if let Some(ptr_pair) = self.find_string(ptr, length, hash) {
      unsafe {
        free_array!(u8, ptr, length);
      }
      ptr_pair
    } else {
      self.allocate_string(ptr, length, hash)
    }
  }

  pub fn copy_string(&mut self, str: &str, start_index: usize, length: usize) -> (StrPtr, GcPtr) {
    let substring = &str[start_index..(start_index + length)];
    let hash = hash_string(substring.as_ptr(), length);
    #[allow(clippy::option_if_let_else)]
    if let Some(ptr_pair) = self.find_string(substring.as_ptr(), length, hash) {
      ptr_pair
    } else {
      let layout = Layout::array::<u8>(length).unwrap();
      let ptr = unsafe { alloc(layout) };
      if ptr.is_null() {
        eprintln!("Reallocation for characters failed");
        handle_alloc_error(layout);
      }
      unsafe {
        copy_nonoverlapping(substring.as_ptr(), ptr, length);
      }
      self.allocate_string(ptr, length, hash)
    }
  }

  fn find_string(&self, chars_ptr: *const u8, length: usize, hash: u32) -> Option<(StrPtr, GcPtr)> {
    self.strings.find_string(chars_ptr, length, hash).map(|interned_gc_ptr| {
      if let HeapString(str_ptr) = unsafe { &*interned_gc_ptr }.object {
        (str_ptr, interned_gc_ptr.cast_mut())
      } else {
        panic!("The only objects that tables can use as keys are strings")
      }
    })
  }

  pub fn mark_array(&mut self, values: &ValueArray) {
    for value in values.iter() {
      self.mark_value(value);
    }
  }

  pub fn mark_object(&mut self, gc_object: &mut GcObject) {
    if !gc_object.is_marked {
      gc_object.set_marked();
      self.grays.push(ptr::from_ref(gc_object));
    }
  }

  fn mark_table_pairs(&mut self, pairs: Vec<(*mut GcObject, *mut Value)>) {
    for (key_ptr, value_ptr) in pairs {
      self.mark_object(unsafe { &mut *key_ptr });
      self.mark_value(unsafe { &*value_ptr });
    }
  }

  fn mark_table(&mut self, table: &mut HashTable) {
    let pairs = table.iter_mut().map(|(k, v)| (ptr::from_mut(k), ptr::from_mut(v))).collect();
    self.mark_table_pairs(pairs);
  }

  pub fn mark_tables(&mut self) {
    let pairs = self.globals.iter_mut().map(|(k, v)| (ptr::from_mut(k), ptr::from_mut(v))).collect();
    self.mark_table_pairs(pairs);
  }

  pub fn mark_value(&mut self, value: &Value) {
    if let Reference(gc_ptr) = value {
      self.mark_object(unsafe { &mut **gc_ptr });
    }
  }

  pub fn sweep(&mut self) {
    let mut prev_opt = None;
    let mut current_ptr = self.objects;

    while !current_ptr.is_null() {
      let this_ptr = current_ptr;
      let gc_obj = unsafe { &mut *current_ptr };
      current_ptr = gc_obj.next;
      if gc_obj.is_marked {
        gc_obj.is_marked = false;
        prev_opt = Some(gc_obj);
      } else {
        if let Some(ref mut prev) = prev_opt {
          prev.next = current_ptr;
        } else {
          self.objects = current_ptr;
        }
        gc_obj.free();
        unsafe {
          drop(Box::from_raw(this_ptr));
        }
      }
    }
  }

  pub fn trace_references(&mut self) {
    while let Some(next_gc_ptr) = self.grays.pop() {
      let next_gc_obj = unsafe { &*next_gc_ptr };
      let blackenables = next_gc_obj.blackenables();
      for bable in blackenables {
        match bable {
          Blackenable::Array(array) => {
            self.mark_array(array);
          },
          Blackenable::Object(obj) => {
            self.mark_object(obj);
          },
          Blackenable::Table(table) => {
            self.mark_table(table);
          },
          Blackenable::Value(value) => {
            self.mark_value(value);
          },
        }
      }
    }
  }
}

fn _free_chars(chars: *const u8, length: u32) {
  let layout = Layout::array::<u8>(length as usize).unwrap();
  unsafe {
    dealloc(chars.cast_mut(), layout);
  }
}

pub fn objs_are_equal(a: &HeapObject, b: &HeapObject) -> bool {
  match (&a, &b) {
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
