use super::gc_object::GcObject;
use super::heap_object::HeapObject::{
  HeapBoundMethod, HeapClass, HeapClosure, HeapFunction, HeapNativeFn, HeapObjInstance, HeapString,
  HeapUpvalue,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
  Boolean(bool),
  Double(f64),
  Nil,
  Reference(*mut GcObject), // TODO: Give GcObject ptr its own newtype
}

impl Value {
  #[must_use]
  pub fn stringify(&self) -> String {
    match self {
      Self::Double(x) => format!("{x}"),
      Self::Boolean(true) => "true".to_string(),
      Self::Boolean(false) => "false".to_string(),
      Self::Nil => "nil".to_string(),
      Self::Reference(gc_ptr) => match unsafe { &**gc_ptr }.object {
        HeapBoundMethod(bound_method_ptr) => unsafe { &*bound_method_ptr }.to_string(),
        HeapClass(class_obj_ptr) => unsafe { &*class_obj_ptr }.to_string(),
        HeapClosure(fn_obj_ptr) => unsafe { &*fn_obj_ptr }.to_string(),
        HeapFunction(fn_ptr) => unsafe { &*fn_ptr }.to_string(),
        HeapNativeFn(native_fn_ptr) => unsafe { &*native_fn_ptr }.to_string(),
        HeapObjInstance(obj_instance_ptr) => unsafe { &*obj_instance_ptr }.to_string(),
        HeapString(str_ptr) => unsafe { &*str_ptr }.to_string(),
        HeapUpvalue(upvalue_ptr) => unsafe { &*upvalue_ptr }.to_string(),
      },
    }
  }
}
