use std::array;
use std::process::exit;
use std::ptr::{self, null_mut};
use std::rc::Rc;
use std::slice;
use std::sync::Mutex;

use crate::chunk::Chunk;

use crate::compiler::compile;

use crate::disassembler::disassemble_instruction;

use crate::object::{GcObject, HeapObject::HeapString, refs_are_equal};

use crate::opcode::OpCode::{
  self, Add, Constant, Divide, Equal, False, Greater, Less, Multiply, Negate, Nil, Not, Return, Subtract,
  True,
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
  inst_ptr: *mut u8,
  objects: Rc<Mutex<*mut GcObject>>,
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
      inst_ptr: null_mut(),
      objects: Rc::new(Mutex::new(null_mut())),
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

    let mut ptr = *self.objects.lock().unwrap();
    while !ptr.is_null() {
      let next = unsafe { (*ptr).next };
      unsafe {
        (*ptr).free();
        drop(Box::from_raw(ptr));
      }
      ptr = next;
    }

    self
  }

  pub fn interpret(&mut self, source: &str) -> Interpretation {
    let mut chunk = Chunk::default();
    self.chunk_opt = Some(&raw const chunk);

    if compile(source, &mut chunk, Rc::clone(&self.objects)) {
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

    macro_rules! read_byte {
      () => {{
        let byte = unsafe { *self.inst_ptr };
        self.inst_ptr = unsafe { self.inst_ptr.add(1) };
        byte
      }};
    }

    macro_rules! read_constant {
      () => {{
        let byte = read_byte!() as usize;
        unsafe { ptr::read((*self.chunk_opt.unwrap()).constants.values.add(byte)) }
      }};
    }

    macro_rules! push_and_win {
      ($x: expr) => {{
        self.push($x);
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

      let ordinal = read_byte!();

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
              if let (HeapString(str1), HeapString(str2)) =
                (unsafe { &(*x.0).object }, unsafe { &(*y.0).object }) =>
            {
              push_and_win!(ReferenceValue(str1.concatenate(str2, &Rc::clone(&self.objects))))
            },
            _ => runtime_error!("Operands must be two numbers or two strings."),
          }
        },
        Some(Constant) => {
          let constant = read_constant!();
          push_and_win!(constant)
        },
        Some(Divide) => binary_op!(Double, /),
        Some(Equal) => {
          let b = self.pop();
          let a = self.pop();
          push_and_win!(Boolean(values_are_equal(a, b)))
        },
        Some(False) => push_and_win!(Boolean(false)),
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
          let x = self.pop();
          push_and_win!(Boolean(is_falsey(&x)))
        },
        Some(Return) => {
          println!("{}", self.pop().stringify());
          Done
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
