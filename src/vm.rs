use std::array;
use std::ptr::{self, null_mut};
use std::sync::LazyLock;
use std::time::Instant;

use crate::core::memory::Freeable;

use crate::compiler::Compiler;
use crate::compiler::function_kind::FunctionKind::Script;

use crate::runtime::gc_object::GcObject;
use crate::runtime::heap::Heap;
use crate::runtime::heap_gc::DEBUG_LOG_GC;
use crate::runtime::heap_object::{ClosureObj, HeapObject::HeapClosure, NativeFnObj};
use crate::runtime::value::Value::{self, Double, Reference};

pub mod interpretation;

mod run;

use interpretation::Interpretation::{self, CompilationError};
use run::FRAMES_MAX;

const STACK_MAX: usize = FRAMES_MAX * (u8::MAX as usize + 1);

static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

#[derive(Debug)]
#[allow(clippy::struct_field_names)]
pub struct CallFrame {
  closure_gc_ptr: *mut GcObject,
  inst_ptr: *mut u8,
  slots_ptr: *mut Value,
}

impl CallFrame {
  fn closure(&self) -> &ClosureObj {
    if let HeapClosure(closure_ptr) = unsafe { &*self.closure_gc_ptr }.object {
      unsafe { &*closure_ptr }
    } else {
      panic!("VM's `closure_gc_ptr` must be a closure!");
    }
  }
}

impl Default for CallFrame {
  fn default() -> Self {
    Self { closure_gc_ptr: null_mut(), inst_ptr: null_mut(), slots_ptr: null_mut() }
  }
}

// Need to hold onto `_stack`, so Rust doesn't overwrite its memory --Jason B. (8/16/26)
pub struct VM {
  compiler: Compiler,
  current_frame_index: usize,
  frames: [CallFrame; FRAMES_MAX],
  instrs_since_last_gc: u32,
  native_fns: Vec<(*mut GcObject, *mut GcObject)>,
  _stack: Box<[Value; STACK_MAX]>,
  stack_addr: *mut Value,
  stack_top: *mut Value,
}

impl VM {
  #[allow(clippy::large_stack_frames)]
  #[must_use]
  pub fn init() -> Self {
    let frames: [CallFrame; FRAMES_MAX] = array::from_fn(|_| CallFrame::default());
    let mut stack: [Value; STACK_MAX] = array::from_fn(|_| Double(0.0));
    let stack_addr = stack.as_mut_ptr();
    let stack_top = stack.as_mut_ptr();

    let mut this = Self {
      compiler: Compiler::default(),
      current_frame_index: 0,
      frames,
      instrs_since_last_gc: 0,
      native_fns: Vec::new(),
      _stack: Box::new(stack),
      stack_addr,
      stack_top,
    };

    let _ = START_TIME;

    this.define_native_fn(
      "clock",
      NativeFnObj(Box::new(|_arg_count, _args_ptr| Double(START_TIME.elapsed().as_secs_f64()))),
    );

    this
  }

  pub fn collect_garbage(&mut self) {
    if DEBUG_LOG_GC {
      println!("-- gc begin");
    }

    self.mark_roots();
    self.compiler.heap.trace_references();
    self.compiler.heap.sweep();

    if DEBUG_LOG_GC {
      println!("-- gc end");
    }
  }

  #[must_use]
  /// # Panics
  ///
  /// When a lock cannot be acquired on the objects for GC'ing.
  pub fn free(&mut self) -> &Self {
    self.compiler.heap.free();
    self.compiler.heap = Heap::new();

    self
  }

  #[must_use]
  pub fn interpret(source: String) -> Interpretation {
    Self::init().interpret_partial(source)
  }

  #[allow(clippy::option_if_let_else)]
  pub fn interpret_partial(&mut self, source: String) -> Interpretation {
    if let Some((_, function_gc_ptr)) = self.compiler.run(source) {
      self.push(Reference(function_gc_ptr));

      let (closure_ptr, closure_gc_ptr) = self.compiler.heap.allocate_closure(function_gc_ptr);
      let _ = self.pop();
      self.push(Reference(closure_gc_ptr));
      let _ = self.call_function_for_error(closure_ptr, closure_gc_ptr, 0, &Script);

      self.run()
    } else {
      CompilationError
    }
  }

  const fn peek(&mut self, distance: usize) -> Value {
    unsafe { ptr::read(self.stack_top.sub(distance + 1)) }
  }

  const fn pop(&mut self) -> Value {
    self.stack_top = unsafe { self.stack_top.sub(1) };
    unsafe { ptr::read(self.stack_top) }
  }

  fn push(&mut self, value: Value) {
    unsafe { *self.stack_top = value };
    self.stack_top = unsafe { self.stack_top.add(1) };
  }

  const fn reset_stack(&mut self) {
    self.stack_top = self.stack_addr;
    self.current_frame_index = 0;
  }
}
