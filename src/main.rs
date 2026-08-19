use std::env;
use std::fs::read_to_string;
use std::io::{Write, stdin, stdout};
use std::process::exit;

use rox::vm::Interpretation::{CompilationError, RuntimeError, Success};
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
