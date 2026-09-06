use crate::runtime::chunk::Chunk;
use crate::runtime::gc_object::GcObject;
use crate::runtime::heap_object::HeapObject::HeapFunction;
use crate::runtime::value::Value::Reference;

use super::opcode::OpCode::{
  self, Add, Class, CloseUpvalue, Closure, Constant, DefineGlobal, Divide, Equal, False, FnCall, GetGlobal,
  GetLocal, GetProperty, GetSuper, GetUpvalue, Greater, Inherit, Invoke, Jump, JumpIfFalse, Less, Loop,
  Method, Multiply, Negate, Nil, Not, Pop, Print, Return, SetGlobal, SetLocal, SetProperty, SetUpvalue,
  Subtract, SuperInvoke, True,
};

// TODO: Does this file really belong in `compiler`?

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
    Some(
      x @ (Class | Constant | DefineGlobal | GetGlobal | GetProperty | GetSuper | Method | SetGlobal
      | SetProperty),
    ) => constant_instruction(&x, chunk, offset),
    Some(
      x @ (Add | CloseUpvalue | Divide | Equal | False | Greater | Inherit | Less | Multiply | Negate | Nil
      | Not | Print | Pop | Return | Subtract | True),
    ) => simple_instruction(&x, offset),
    Some(x @ (FnCall | GetLocal | GetUpvalue | SetLocal | SetUpvalue)) => byte_instruction(&x, chunk, offset),
    Some(x @ (Jump | JumpIfFalse | Loop)) => jump_instruction(&x, 1, chunk, offset),
    Some(x @ Closure) => closure_instruction(&x, chunk, offset),
    Some(x @ (Invoke | SuperInvoke)) => invoke_instruction(&x, chunk, offset),
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

fn closure_instruction(op_code: &OpCode, chunk: &Chunk, offset: usize) -> usize {
  let mut wip_offset = offset + 1;
  let constant_index = unsafe { *chunk.op_codes.add(wip_offset) };
  wip_offset += 1;

  let value_obj = unsafe { &*chunk.constants.values.add(constant_index as usize) };
  let value_str = value_obj.stringify();
  println!("{op_code:<16?} {constant_index:>4} {value_str}");

  if let Reference(gc_ptr) = value_obj
    && let GcObject { object, .. } = unsafe { &**gc_ptr }
    && let HeapFunction(function_obj_ptr) = object
  {
    let upvalue_count = unsafe { &**function_obj_ptr }.upvalue_count();
    for _ in 0..upvalue_count {
      let is_local = unsafe { &*chunk.op_codes.add(wip_offset) };
      wip_offset += 1;
      let upvalue_index = unsafe { &*chunk.op_codes.add(wip_offset) };
      wip_offset += 1;

      let locality = if *is_local == 1 { "local" } else { "upvalue" };
      println!("{}      |                     {locality} {upvalue_index}", wip_offset - 2);
    }
    wip_offset
  } else {
    panic!("Bad instruction!");
  }
}

fn constant_instruction(op_code: &OpCode, chunk: &Chunk, offset: usize) -> usize {
  let constant_index = unsafe { *chunk.op_codes.add(offset + 1) };
  let value_str = unsafe { (*chunk.constants.values.add(constant_index as usize)).stringify() };
  println!("{op_code:<16?} {constant_index:>4} '{value_str}'");
  offset + 2
}

fn invoke_instruction(op_code: &OpCode, chunk: &Chunk, offset: usize) -> usize {
  let constant_index = unsafe { *chunk.op_codes.add(offset + 1) };
  let arg_count = unsafe { *chunk.op_codes.add(offset + 2) };
  let value_str = unsafe { (*chunk.constants.values.add(constant_index as usize)).stringify() };
  println!("{op_code:<16?} ({arg_count} args) {constant_index:>4} '{value_str}'");
  offset + 3
}

fn jump_instruction(op_code: &OpCode, sign: u32, chunk: &Chunk, offset: usize) -> usize {
  let upper_bits = unsafe { u16::from(*chunk.op_codes.add(offset + 1)) } << 8;
  let lower_bits = unsafe { u16::from(*chunk.op_codes.add(offset + 2)) };
  let jump_target = upper_bits | lower_bits;
  println!("{op_code:<16?} {offset:>4} -> {}", offset + 3 + (sign as usize) * (jump_target as usize));
  offset + 3
}

fn simple_instruction(op_code: &OpCode, offset: usize) -> usize {
  println!("{op_code:?}");
  offset + 1
}
