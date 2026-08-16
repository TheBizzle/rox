use std::env;

use tokio::io::{stdin, stdout};

use rox::chunk::Chunk;

use rox::disassembler::disassemble_chunk;

use rox::opcode::OpCode::{Constant, Return};

use rox::value::Value::Double;

#[tokio::main]
async fn main() {
  let _args: Vec<String> = env::args().collect();
  let _stdin = stdin();
  let _stdout = stdout();

  let mut chunk = Chunk::default();

  let constant_index = chunk.add_constant(Double(1.2));
  chunk.write(Constant as u8, 123);
  chunk.write(constant_index, 123);

  chunk.write(Return as u8, 123);
  disassemble_chunk(&chunk, "test chunk");

  let _ = chunk.free();
}
