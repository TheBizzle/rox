use std::env;

use tokio::io::{stdin, stdout};

use rox::chunk::Chunk;

use rox::disassembler::disassemble_chunk;

use rox::opcode::OpCode::{Add, Constant, Divide, Negate, Return};

use rox::value::Value::Double;

use rox::vm::VM;

#[tokio::main]
async fn main() {
  let _args: Vec<String> = env::args().collect();
  let _stdin = stdin();
  let _stdout = stdout();

  let mut vm = VM::init();

  let mut chunk = Chunk::default();

  let constant_index = chunk.add_constant(Double(1.2));
  chunk.write(Constant as u8, 123);
  chunk.write(constant_index, 123);

  let constant_index2 = chunk.add_constant(Double(1.8));
  chunk.write(Constant as u8, 123);
  chunk.write(constant_index2, 123);

  chunk.write(Add as u8, 123);

  let constant_index3 = chunk.add_constant(Double(3.0));
  chunk.write(Constant as u8, 123);
  chunk.write(constant_index3, 123);

  chunk.write(Divide as u8, 123);
  chunk.write(Negate as u8, 123);

  chunk.write(Return as u8, 123);

  disassemble_chunk(&chunk, "test chunk");

  vm.interpret(&chunk);

  let _ = vm.free();
  let _ = chunk.free();
}
