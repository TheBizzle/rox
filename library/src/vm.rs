use std::array;
use std::ptr;
use std::sync::LazyLock;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::core::memory::Freeable;
use crate::core::output::Output;

use crate::compiler::Compiler;
use crate::compiler::function_kind::FunctionKind::Script;

use crate::runtime::gc_object::{GcObject, GcPtr};
use crate::runtime::heap::Heap;
use crate::runtime::heap_gc::DEBUG_LOG_GC;
use crate::runtime::heap_object::NativeFnObj;
use crate::runtime::value::Value::{self, Double, Reference};

pub mod interpretation;

mod call_frame;
mod deserialize;
mod run;
mod serialize;

use call_frame::CallFrame;
use deserialize::deserialize;
use interpretation::Interpretation::{self, CompilationError};
use run::FRAMES_MAX;
use serialize::serialize_root;

const STACK_MAX: usize = FRAMES_MAX * (u8::MAX as usize + 1);

static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

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
  #[must_use]
  pub fn init() -> Self {
    Self::initialize(Compiler::default())
  }

  #[must_use]
  pub fn load_and_run(serialized: &str) -> (Interpretation, Vec<Output>) {
    let mut compiler = deserialize(serialized);
    let main_fn_gc_ptr = compiler.get_root_fn_ptr();
    let mut this = Self::initialize(compiler);

    this.push(Reference(GcPtr(main_fn_gc_ptr)));
    let (closure_ptr, closure_gc_ptr) = this.compiler.heap.allocate_closure(main_fn_gc_ptr);
    let _ = this.pop();
    this.push(Reference(GcPtr(closure_gc_ptr)));
    let _ = this.call_function_for_error(closure_ptr, closure_gc_ptr, 0, &Script);

    this.run()
  }

  #[allow(clippy::large_stack_frames)]
  #[inline]
  fn initialize(compiler: Compiler) -> Self {
    let frames: [CallFrame; FRAMES_MAX] = array::from_fn(|_| CallFrame::default());
    let mut stack: [Value; STACK_MAX] = array::from_fn(|_| Double(0.0));
    let stack_addr = stack.as_mut_ptr();
    let stack_top = stack.as_mut_ptr();

    let mut this = Self {
      compiler,
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
  pub fn interpret(source: String) -> (Interpretation, Vec<Output>) {
    Self::init().interpret_partial(source)
  }

  #[allow(clippy::option_if_let_else)]
  pub fn interpret_partial(&mut self, source: String) -> (Interpretation, Vec<Output>) {
    if let Some((_, function_gc_ptr)) = self.compiler.run(source) {
      self.push(Reference(GcPtr(function_gc_ptr)));

      let (closure_ptr, closure_gc_ptr) = self.compiler.heap.allocate_closure(function_gc_ptr);
      let _ = self.pop();
      self.push(Reference(GcPtr(closure_gc_ptr)));
      let _ = self.call_function_for_error(closure_ptr, closure_gc_ptr, 0, &Script);

      self.run()
    } else {
      (CompilationError, self.compiler.take_wasm_output())
    }
  }

  /// # Errors
  /// When there was a compilation error
  pub fn serialize(source: String) -> Result<String, Vec<Output>> {
    let mut compiler = Compiler::default();
    if let Some((_, function_gc_ptr)) = compiler.run(source) {
      Ok(serialize_root(function_gc_ptr))
    } else {
      Err(compiler.take_wasm_output())
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
