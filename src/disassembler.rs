use crate::chunk::Chunk;

use crate::opcode::OpCode::{
  self, Add, Constant, DefineGlobal, Divide, Equal, False, GetGlobal, GetLocal, Greater, Less, Multiply,
  Negate, Nil, Not, Pop, Print, Return, SetGlobal, SetLocal, Subtract, True,
};

pub fn disassemble_chunk(chunk: &Chunk, name: &str) {
  println!("== {name} ==");

  let mut offset = 0;
  while offset < chunk.count {
    offset = disassemble_instruction(chunk, offset);
  }
}

#[allow(clippy::must_use_candidate)]
pub fn disassemble_instruction(chunk: &Chunk, offset: usize) -> usize {
  print!("{offset:04} ");

  if offset > 0 && unsafe { *chunk.line_nums.add(offset) } == unsafe { *chunk.line_nums.add(offset - 1) } {
    print!("   | ");
  } else {
    print!("{:4} ", unsafe { *chunk.line_nums.add(offset) });
  }

  let ordinal = unsafe { *chunk.op_codes.add(offset) };
  match OpCode::from_repr(ordinal) {
    Some(x @ (Constant | DefineGlobal | GetGlobal | SetGlobal)) => constant_instruction(&x, chunk, offset),
    Some(
      x @ (Add | Divide | Equal | False | Greater | Less | Multiply | Negate | Nil | Not | Print | Pop
      | Return | Subtract | True),
    ) => simple_instruction(&x, offset),
    Some(x @ (GetLocal | SetLocal)) => byte_instruction(&x, chunk, offset),
    None => {
      println!("Unknown opcode: {chunk:?} | {offset}");
      offset + 1
    },
  }
}

fn byte_instruction(op_code: &OpCode, chunk: &Chunk, offset: usize) -> usize {
  let slot_num = unsafe { *chunk.op_codes.add(offset + 1) };
  println!("{op_code:<16?} {slot_num:>4}");
  offset + 2
}

fn constant_instruction(op_code: &OpCode, chunk: &Chunk, offset: usize) -> usize {
  let constant_index = unsafe { *chunk.op_codes.add(offset + 1) };
  let value_str = unsafe { (*chunk.constants.values.add(constant_index as usize)).stringify() };
  println!("{op_code:<16?} {constant_index:>4} '{value_str}'");
  offset + 2
}

fn simple_instruction(op_code: &OpCode, offset: usize) -> usize {
  println!("{op_code:?}");
  offset + 1
}
