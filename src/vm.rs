use std::array;
use std::process::exit;
use std::ptr::{self, null_mut};
use std::slice;

use crate::chunk::Chunk;

use crate::compiler::compile;

use crate::disassembler::disassemble_instruction;

use crate::gc::{Gc, HeapObject::HeapString, Reference, refs_are_equal};

use crate::opcode::OpCode::{
  self, Add, Constant, DefineGlobal, Divide, Equal, False, GetGlobal, GetLocal, Greater, Less, Multiply,
  Negate, Nil, Not, Pop, Print, Return, SetGlobal, SetLocal, Subtract, True,
};

use crate::value::Value::{self, Boolean, Double, Nil as NilValue, ReferenceValue};

const IS_DEBUGGING: bool = true;
const STACK_MAX: usize = 256;

#[derive(Eq, PartialEq)]
pub enum Interpretation {
  CompilationError,
  RuntimeError,
  Success,
}
use Interpretation::{CompilationError, RuntimeError, Success};

// Need to hold onto `_stack`, so Rust doesn't overwrite its memory --Jason B. (8/16/26)
pub struct VM {
  chunk_opt: Option<*const Chunk>,
  gc: Gc,
  inst_ptr: *mut u8,
  _stack: Box<[Value; STACK_MAX]>,
  stack_addr: *mut Value,
  stack_top: *mut Value,
}

impl VM {
  #[must_use]
  pub fn init() -> Self {
    let mut stack: [Value; STACK_MAX] = array::from_fn(|_| Double(0.0));
    let stack_addr = stack.as_mut_ptr();
    let stack_top = stack.as_mut_ptr();
    Self {
      chunk_opt: None,
      gc: Gc::new(),
      inst_ptr: null_mut(),
      _stack: Box::new(stack),
      stack_addr,
      stack_top,
    }
  }

  #[must_use]
  /// # Panics
  ///
  /// When a lock cannot be acquired on the objects for GC'ing.
  pub fn free(&mut self) -> &Self {
    self.chunk_opt = None;
    self.gc.free();
    self.gc = Gc::new();

    self
  }

  pub fn interpret(&mut self, source: &str) -> Interpretation {
    let mut chunk = Chunk::default();
    self.chunk_opt = Some(&raw const chunk);

    if compile(source, &mut chunk, &mut self.gc) {
      self.inst_ptr = chunk.op_codes;
      let result = self.run();
      let _ = chunk.free();
      result
    } else {
      let _ = chunk.free();
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
  }

  #[allow(clippy::too_many_lines)]
  fn run(&mut self) -> Interpretation {
    enum ProgressState {
      Continue,
      Done,
      Error,
    }
    use ProgressState::{Continue, Done, Error};

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
        let byte = unsafe { *self.inst_ptr };
        self.inst_ptr = unsafe { self.inst_ptr.add(1) };
        byte
      }};
    }

    macro_rules! read_constant {
      () => {{
        let byte = read_u8!() as usize;
        unsafe { ptr::read((*self.chunk_opt.unwrap()).constants.values.add(byte)) }
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

        let chunk = unsafe { &*self.chunk_opt.unwrap() };
        let offset = unsafe { self.inst_ptr.offset_from(chunk.op_codes).cast_unsigned() };
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
          let value = unsafe { &*self.stack_addr.add(slot_num as usize) }.clone();
          push_and_win!(value)
        },
        Some(Greater) => binary_op!(Boolean, >),

        Some(Less) => binary_op!(Boolean, <),

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

        Some(Return) => Done,

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
          let value = self.peek(0);
          unsafe { *self.stack_top.add(slot_num as usize) = value };
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

  fn runtime_error_impl(&mut self, args: std::fmt::Arguments) {
    eprintln!("{args}");

    let line = unsafe {
      let chunk = &*self.chunk_opt.unwrap();
      let instruction = self.inst_ptr.offset_from(chunk.op_codes).cast_unsigned() - 1;
      *chunk.line_nums.add(instruction)
    };

    eprintln!("[line {line}] in script");

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
