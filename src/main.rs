use std::env;
use std::fs::read_to_string;
use std::io::{Write, stdin, stdout};
use std::process::exit;

use rox::chunk::Chunk;

use rox::compiler::compile;

use rox::disassembler::disassemble_chunk;

use rox::opcode::OpCode::{Add, Constant, Divide, Negate, Return};

use rox::value::Value::Double;

use rox::vm::Interpretation::{self, CompilationError, RuntimeError, Success};
use rox::vm::VM;

#[tokio::main]
async fn main() {
  let args: Vec<String> = env::args().collect();

  let mut vm = VM::init();

  match &args[..] {
    [_] => run_repl(&vm),
    [_, filepath] => run_file(&vm, filepath),
    _ => {
      eprintln!("Usage: rox [path]");
      exit(64)
    },
  }

  let _ = vm.free();

  exit(0);
}

fn run_repl(vm: &VM) {
  let mut input = String::with_capacity(1024);
  let stdin = stdin();
  let mut stdout = stdout();
  loop {
    print!("> ");
    stdout.flush().unwrap();
    if let Err(error) = stdin.read_line(&mut input) {
      println!("{error}");
      exit(65);
    }
    interpret(vm, &input);
  }
}

fn run_file(vm: &VM, filepath: &str) {
  match read_to_string(filepath) {
    Err(_) => {
      eprintln!("Could not read file \"{filepath}\".");
      exit(74);
    },
    Ok(source) => {
      let result = interpret(vm, &source);
      drop(source);

      match result {
        CompilationError => exit(65),
        RuntimeError => exit(70),
        Success => {},
      }
    },
  }
}

fn interpret(_vm: &VM, source: &str) -> Interpretation {
  compile(source);
  Success
}

fn _fraudulent_main() {
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
