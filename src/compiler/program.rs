use std::collections::HashMap;

use crate::runtime::chunk::Chunk;
use crate::runtime::gc_object::GcObject;
use crate::runtime::heap_object::FunctionObj;
use crate::runtime::heap_object::HeapObject::HeapFunction;

use super::function_kind::FunctionKind::{self, Initializer, Method};
use super::opcode::OpCode::{self, CloseUpvalue, Pop};

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Upvalue {
  pub(super) index: u8,
  pub(super) is_local: bool,
}

#[derive(Debug)]
pub(super) enum LocalVar {
  GlobalFunction,
  LocalBinding { name: String, depth_opt: Option<u8>, is_captured: bool },
}
use LocalVar::{GlobalFunction, LocalBinding};

impl LocalVar {
  const fn depth_opt(&self) -> Option<&u8> {
    match self {
      GlobalFunction => None,
      LocalBinding { depth_opt, .. } => depth_opt.as_ref(),
    }
  }

  const fn is_captured(&self) -> bool {
    match self {
      GlobalFunction => false,
      LocalBinding { is_captured, .. } => *is_captured,
    }
  }

  pub(super) const fn mark_captured(&mut self) {
    match self {
      GlobalFunction => {},
      LocalBinding { is_captured, .. } => {
        *is_captured = true;
      },
    }
  }
}

pub(super) struct Program {
  pub(super) function_gc_ptr: *mut GcObject,
  pub(super) function_kind: FunctionKind,

  pub(super) ident_byte_cache: HashMap<*mut GcObject, u8>,

  pub(super) local_var_opts: Box<[Option<LocalVar>; u8::MAX as usize + 1]>,
  pub(super) local_var_count: u16,
  pub(super) scope_depth: u8,

  pub(super) upvalues: [Option<Upvalue>; u8::MAX as usize + 1],
}

impl Program {
  #[must_use]
  pub fn new(function_gc_ptr: *mut GcObject, function_kind: FunctionKind) -> Self {
    let size = u8::MAX as usize + 1;
    let mut v = Vec::with_capacity(size);
    v.resize_with(size, || None);

    let first_binding = if function_kind == Method || function_kind == Initializer {
      LocalBinding { name: "this".to_string(), depth_opt: Some(0), is_captured: false }
    } else {
      GlobalFunction
    };

    v[0] = Some(first_binding);

    let local_var_opts = v.try_into().expect("Length must be exactly `u8::MAX + 1`");

    let mut v2 = Vec::with_capacity(size);
    v2.resize_with(size, || None);
    let upvalues = v2.try_into().expect("Length must be exactly `u8::MAX + 1`");

    Self {
      function_gc_ptr,
      function_kind,
      ident_byte_cache: HashMap::new(),
      local_var_opts,
      local_var_count: 1,
      scope_depth: 0,
      upvalues,
    }
  }

  pub const fn begin_scope(&mut self) {
    self.scope_depth += 1;
  }

  pub fn chunk(&mut self) -> &mut Chunk {
    self.function().chunk_mut()
  }

  pub fn end_scope(&mut self) -> Vec<OpCode> {
    self.scope_depth -= 1;

    let mut op_codes = Vec::new();

    while self.local_var_count > 0
      && let index = (self.local_var_count - 1) as usize
      && let local_var = self.local_var_opts[index].as_ref().unwrap()
      && let Some(depth) = local_var.depth_opt()
      && depth > &self.scope_depth
    {
      if local_var.is_captured() {
        op_codes.push(CloseUpvalue);
      } else {
        op_codes.push(Pop);
      }
      self.local_var_count -= 1;
    }

    op_codes
  }

  pub(super) fn mark_latest_var_initialized(&mut self) {
    if self.scope_depth != 0 {
      let index = (self.local_var_count - 1) as usize;
      if let LocalBinding { depth_opt, .. } = self.local_var_opts[index].as_mut().unwrap() {
        let _ = depth_opt.insert(self.scope_depth);
      }
    }
  }

  pub(super) fn function(&mut self) -> &mut FunctionObj {
    match unsafe { &mut *self.function_gc_ptr }.object {
      HeapFunction(fn_ptr) => unsafe { &mut *fn_ptr },
      _ => {
        panic!("The program's `function_gc_ptr` is only allowed to be a function!");
      },
    }
  }
}
