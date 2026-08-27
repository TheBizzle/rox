use std::array;
use std::fmt::Arguments;
use std::process::exit;
use std::ptr::{self, null_mut};
use std::slice;
use std::sync::LazyLock;
use std::time::Instant;

use crate::compiler::{
  FunctionKind::{self, Function, Script},
  compile,
};

use crate::disassembler::disassemble_instruction;

use crate::gc::FunctionObj::{self, MainScript, UserDefined};
use crate::gc::HeapObject::{HeapFunction, HeapNativeFn, HeapString};
use crate::gc::{Gc, NativeFnObj, Reference, refs_are_equal};

use crate::opcode::OpCode::{
  self, Add, Constant, DefineGlobal, Divide, Equal, False, FnCall, GetGlobal, GetLocal, Greater, Jump,
  JumpIfFalse, Less, Loop, Multiply, Negate, Nil, Not, Pop, Print, Return, SetGlobal, SetLocal, Subtract,
  True,
};

use crate::value::Value::{self, Boolean, Double, Nil as NilValue, ReferenceValue};

const FRAMES_MAX: usize = 64;
const IS_DEBUGGING: bool = true;
const STACK_MAX: usize = FRAMES_MAX * (u8::MAX as usize + 1);

static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

#[derive(Eq, PartialEq)]
pub enum Interpretation {
  CompilationError,
  RuntimeError,
  Success,
}
use Interpretation::{CompilationError, RuntimeError, Success};

#[derive(Debug)]
#[allow(clippy::struct_field_names)]
pub struct CallFrame {
  function_ptr: *mut FunctionObj,
  inst_ptr: *mut u8,
  slots_ptr: *mut Value,
}

impl Default for CallFrame {
  fn default() -> Self {
    Self { function_ptr: null_mut(), inst_ptr: null_mut(), slots_ptr: null_mut() }
  }
}

// Need to hold onto `_stack`, so Rust doesn't overwrite its memory --Jason B. (8/16/26)
pub struct VM {
  current_frame_index: usize,
  frames: [CallFrame; FRAMES_MAX],
  gc: Gc,
  _stack: Box<[Value; STACK_MAX]>,
  stack_addr: *mut Value,
  stack_top: *mut Value,
}

enum ProgressState {
  Continue,
  Done,
  Error,
}
use ProgressState::{Continue, Done, Error};

impl VM {
  #[allow(clippy::large_stack_frames)]
  #[must_use]
  pub fn init() -> Self {
    let frames: [CallFrame; FRAMES_MAX] = array::from_fn(|_| CallFrame::default());
    let mut stack: [Value; STACK_MAX] = array::from_fn(|_| Double(0.0));
    let stack_addr = stack.as_mut_ptr();
    let stack_top = stack.as_mut_ptr();

    let mut this = Self {
      current_frame_index: 0,
      frames,
      gc: Gc::new(),
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

  #[must_use]
  /// # Panics
  ///
  /// When a lock cannot be acquired on the objects for GC'ing.
  pub fn free(&mut self) -> &Self {
    self.gc.free();
    self.gc = Gc::new();

    self
  }

  #[allow(clippy::option_if_let_else)]
  pub fn interpret(&mut self, source: &str) -> Interpretation {
    if let Some(fn_ptr) = compile(source, &mut self.gc) {
      self.push(ReferenceValue(Reference(HeapFunction(fn_ptr))));

      let _ = self.call_function_for_error(fn_ptr, 0, &Script);

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

  #[allow(clippy::too_many_lines)]
  fn run(&mut self) -> Interpretation {
    macro_rules! runtime_error {
      ($($arg: tt)*) => {{
        self.runtime_error_impl(format_args!($($arg)*));
        Error
      }};
    }

    macro_rules! binary_op {
      ($value_type: tt, $op: tt) => {{
        match (self.peek(0), self.peek(1)) {
          (Double(b), Double(a)) => {
            let _      = self.pop();
            let _      = self.pop();
            let result = $value_type(a $op b);
            self.push(result);
            Continue
          },
          _ => {
            runtime_error!("Operands must be numbers.")
          }

        }
      }};
    }

    macro_rules! read_u8 {
      () => {{
        let current = &mut self.frames[self.current_frame_index];
        let byte = unsafe { *current.inst_ptr };
        current.inst_ptr = unsafe { current.inst_ptr.add(1) };
        byte
      }};
    }

    macro_rules! read_u16 {
      () => {{ u16::from_be_bytes([read_u8!(), read_u8!()]) }};
    }

    macro_rules! read_constant {
      () => {{
        let byte = read_u8!() as usize;
        let current = &mut self.frames[self.current_frame_index];
        let chunk = unsafe { &*current.function_ptr }.chunk();
        unsafe { ptr::read(chunk.constants.values.add(byte)) }
      }};
    }

    macro_rules! read_string {
      () => {{
        if let ReferenceValue(Reference(HeapString(str_obj_ptr))) = read_constant!() {
          str_obj_ptr
        } else {
          exit(1);
        }
      }};
    }

    macro_rules! push_and_win {
      ($x: expr) => {{
        let x = $x;
        self.push(x);
        Continue
      }};
    }

    loop {
      if IS_DEBUGGING {
        print!("          ");
        let size = unsafe { self.stack_top.offset_from(self.stack_addr) };
        let stack = unsafe { slice::from_raw_parts(self.stack_addr, size.cast_unsigned()) };
        for slot in stack {
          print!("[ {} ]", (*slot).stringify());
        }
        println!();

        let current = &self.frames[self.current_frame_index];
        let chunk = unsafe { &*current.function_ptr }.chunk();
        let offset = unsafe { current.inst_ptr.offset_from(chunk.op_codes).cast_unsigned() };
        disassemble_instruction(chunk, offset);
      }

      let ordinal = read_u8!();

      let progress_state = match OpCode::from_repr(ordinal) {
        Some(Add) => {
          let a = self.peek(1);
          let b = self.peek(0);

          match (a, b) {
            (Double(x), Double(y)) => {
              let _ = self.pop();
              let _ = self.pop();
              push_and_win!(Double(x + y))
            },
            #[allow(irrefutable_let_patterns)]
            (ReferenceValue(x), ReferenceValue(y))
              if let (HeapString(str1), HeapString(str2)) = (x.0.clone(), y.0.clone()) =>
            {
              push_and_win!(ReferenceValue(self.gc.concatenate_strings(str1, str2)))
            },
            _ => runtime_error!("Operands must be two numbers or two strings."),
          }
        },

        Some(Constant) => {
          push_and_win!(read_constant!())
        },

        Some(DefineGlobal) => {
          let value = self.peek(0);
          self.gc.globals.set(read_string!(), value);
          let _ = self.pop();
          Continue
        },
        Some(Divide) => binary_op!(Double, /),

        Some(Equal) => {
          let b = self.pop();
          let a = self.pop();
          push_and_win!(Boolean(values_are_equal(a, b)))
        },

        Some(False) => push_and_win!(Boolean(false)),
        Some(FnCall) => {
          let arg_count = read_u8!();
          let value = self.peek(arg_count as usize);
          self.call_value_for_error(&value, arg_count).unwrap_or(Continue)
        },

        Some(GetGlobal) => {
          let name_ptr = read_string!();
          if let Some(r) = self.gc.globals.get(name_ptr) {
            let value = unsafe { &*r }.clone();
            push_and_win!(value)
          } else {
            let name = unsafe { &*name_ptr };
            runtime_error!("Undefined variable '{:?}'.", name.chars)
          }
        },
        Some(GetLocal) => {
          let slot_num = read_u8!();
          let slots_ptr = self.frames[self.current_frame_index].slots_ptr;
          let value = unsafe { &*slots_ptr.add(slot_num as usize) }.clone();
          push_and_win!(value)
        },
        Some(Greater) => binary_op!(Boolean, >),

        Some(Jump) => {
          let offset = read_u16!();
          let current = &mut self.frames[self.current_frame_index];
          unsafe {
            current.inst_ptr = current.inst_ptr.add(offset as usize);
          }
          Continue
        },
        Some(JumpIfFalse) => {
          let offset = read_u16!() as usize;
          let value = self.peek(0);
          let current = &mut self.frames[self.current_frame_index];
          if is_falsey(&value) {
            current.inst_ptr = unsafe { current.inst_ptr.add(offset) };
          }
          Continue
        },

        Some(Less) => binary_op!(Boolean, <),
        Some(Loop) => {
          let offset = read_u16!() as usize;
          let current = &mut self.frames[self.current_frame_index];
          current.inst_ptr = unsafe { current.inst_ptr.sub(offset) };
          Continue
        },

        Some(Multiply) => binary_op!(Double, *),

        Some(Negate) => {
          if let Double(x) = self.peek(0) {
            let _ = self.pop();
            push_and_win!(Double(-x))
          } else {
            runtime_error!("Operand must be a number.")
          }
        },
        Some(Nil) => push_and_win!(Value::Nil),
        Some(Not) => {
          push_and_win!(Boolean(is_falsey(&self.pop())))
        },

        Some(Pop) => {
          let _ = self.pop();
          Continue
        },
        Some(Print) => {
          println!("{}", self.pop().stringify());
          Continue
        },

        Some(Return) => {
          let result = self.pop();
          if self.current_frame_index == 0 {
            let _ = self.pop();
            Done
          } else {
            self.stack_top = self.frames[self.current_frame_index].slots_ptr;
            self.push(result);
            self.current_frame_index -= 1;
            Continue
          }
        },

        Some(SetGlobal) => {
          let name_ptr = read_string!();
          let name = unsafe { &*name_ptr };
          let value = self.peek(0);
          let is_binding_new = self.gc.globals.set(name, value);

          if is_binding_new {
            self.gc.globals.delete(name);
            runtime_error!("Undefined variable '{:?}'.", name.chars)
          } else {
            Continue
          }
        },
        Some(SetLocal) => {
          let slot_num = read_u8!();
          let slots_ptr = self.frames[self.current_frame_index].slots_ptr;
          let value = self.peek(0);
          unsafe { *slots_ptr.add(slot_num as usize) = value };
          Continue
        },
        Some(Subtract) => binary_op!(Double, -),

        Some(True) => push_and_win!(Boolean(true)),

        None => {
          println!("Unknown instruction enum ordinal: {ordinal}");
          exit(1);
        },
      };

      match progress_state {
        Error => {
          return RuntimeError;
        },
        Done => {
          return Success;
        },
        Continue => {},
      }
    }
  }

  fn call_function_for_error(
    &mut self, func_ptr: *mut FunctionObj, arg_count: u8, function_kind: &FunctionKind,
  ) -> Option<ProgressState> {
    let callee = unsafe { &*func_ptr };

    if u32::from(arg_count) == callee.arity() {
      if self.current_frame_index == (FRAMES_MAX - 1) {
        self.runtime_error_impl(format_args!("Stack overflow."));
        Some(Error)
      } else {
        if function_kind == &Function {
          self.current_frame_index += 1;
        }
        let current = &mut self.frames[self.current_frame_index];

        current.function_ptr = func_ptr;
        current.inst_ptr = callee.chunk().op_codes;
        current.slots_ptr = unsafe { self.stack_top.sub((arg_count + 1) as usize) };

        None
      }
    } else {
      self.runtime_error_impl(format_args!("Expected {} arguments but got {arg_count}.", callee.arity()));
      Some(Error)
    }
  }

  fn call_value_for_error(&mut self, callee: &Value, arg_count: u8) -> Option<ProgressState> {
    if let ReferenceValue(Reference(HeapFunction(func_ptr))) = callee
      && !func_ptr.is_null()
    {
      self.call_function_for_error(*func_ptr, arg_count, &Function)
    } else if let ReferenceValue(Reference(HeapNativeFn(native_fn_ptr))) = callee
      && !native_fn_ptr.is_null()
    {
      let native_fn = unsafe { &**native_fn_ptr };
      let result = native_fn.invoke(arg_count, unsafe { self.stack_top.sub(arg_count as usize) });
      unsafe { self.stack_top = self.stack_top.sub((arg_count + 1) as usize) };
      self.push(result);
      None
    } else {
      self.runtime_error_impl(format_args!("Can only call functions and classes."));
      Some(Error)
    }
  }

  pub fn define_native_fn(&mut self, name: &str, native_fn: NativeFnObj) {
    let name_ptr = self.gc.copy_string(name, 0, name.len());
    let native_fn_ptr = self.gc.allocate_native_fn(native_fn);

    let name_value = ReferenceValue(Reference(HeapString(name_ptr)));
    let native_fn_value = ReferenceValue(Reference(HeapNativeFn(native_fn_ptr)));
    self.push(name_value);
    self.push(native_fn_value.clone());

    let key = unsafe { &*name_ptr };
    self.gc.globals.set(key, native_fn_value);

    self.pop();
    self.pop();
  }

  fn runtime_error_impl(&mut self, args: Arguments) {
    eprintln!("{args}");

    for i in (0..=self.current_frame_index).rev() {
      let frame = &mut self.frames[i];
      let function = unsafe { &*frame.function_ptr };
      let op_codes_ptr = function.chunk().op_codes;
      let instruction = unsafe { frame.inst_ptr.offset_from(op_codes_ptr) }.cast_unsigned() - 1;

      let suffix = match function {
        MainScript { .. } => "script".to_string(),
        UserDefined { name_ptr, .. } => {
          let string = unsafe { &**name_ptr }.to_string();
          format!("{}()", &string[1..(string.len() - 1)])
        },
      };

      let line_num = unsafe { &*function.chunk().line_nums.add(instruction) };
      eprintln!("[line {line_num}] in {suffix}");
    }

    self.reset_stack();
  }
}

const fn is_falsey(value: &Value) -> bool {
  matches!(value, NilValue | Boolean(false))
}

fn values_are_equal(a: Value, b: Value) -> bool {
  match (a, b) {
    (Boolean(x), Boolean(y)) => x == y,
    (Double(x), Double(y)) => (x - y).abs() < 1e-9,
    (ReferenceValue(x), ReferenceValue(y)) => refs_are_equal(&x, &y),
    (NilValue, NilValue) => true,
    _ => false,
  }
}
