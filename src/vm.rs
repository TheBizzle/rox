use std::array;
use std::process::exit;
use std::ptr::{self, null_mut};
use std::slice;

use crate::chunk::Chunk;

use crate::disassembler::disassemble_instruction;

use crate::opcode::OpCode::{self, Add, Constant, Divide, Multiply, Negate, Return, Subtract};

use crate::value::Value::{self, Double};

const IS_DEBUGGING: bool = true;
const STACK_MAX: usize = 256;

pub enum Interpretation {
  CompilationError,
  RuntimeError,
  Success,
}

// Need to hold onto `_stack`, so Rust doesn't overwrite its memory --Jason B. (8/16/26)
pub struct VM {
  chunk_opt: Option<*const Chunk>,
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
    Self { chunk_opt: None, inst_ptr: null_mut(), _stack: Box::new(stack), stack_addr, stack_top }
  }

  #[must_use]
  pub const fn free(&mut self) -> &Self {
    self.chunk_opt = None;

    self
  }

  pub fn interpret(&mut self, chunk: &Chunk) -> Interpretation {
    self.chunk_opt = Some(chunk);
    self.inst_ptr = chunk.op_codes;
    self.run()
  }

  const fn pop(&mut self) -> Value {
    self.stack_top = unsafe { self.stack_top.sub(1) };
    unsafe { ptr::read(self.stack_top) }
  }

  fn push(&mut self, value: Value) {
    unsafe { *self.stack_top = value };
    self.stack_top = unsafe { self.stack_top.add(1) };
  }

  const fn _reset_stack(&mut self) {
    self.stack_top = self.stack_addr;
  }

  fn run(&mut self) -> Interpretation {
    macro_rules! binary_op {
        ($op: tt) => {{
            let b = self.pop();
            let a = self.pop();
            let result =
              match (a, b) {
                (Double(x), Double(y)) => Double(x $op y)
              };
            self.push(result);
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
      match OpCode::from_repr(ordinal) {
        Some(Add) => binary_op!(+),
        Some(Constant) => {
          let constant = read_constant!();
          self.push(constant);
        },
        Some(Subtract) => binary_op!(-),
        Some(Divide) => binary_op!(/),
        Some(Multiply) => binary_op!(*),
        Some(Negate) => {
          let new_value = match self.pop() {
            Double(x) => Double(-x),
          };
          self.push(new_value);
        },
        Some(Return) => {
          println!("{}", self.pop().stringify());
          return Interpretation::Success;
        },
        None => {
          println!("Unknown instruction enum ordinal: {ordinal}");
          exit(1);
        },
      }
    }
  }
}
