use std::env;
use std::fs::read_to_string;
use std::io::{Write, stdin, stdout};
use std::process::exit;

use rox::vm::VM;
use rox::vm::interpretation::Interpretation::{CompilationError, RuntimeError, Success};

#[tokio::main]
async fn main() {
  let args: Vec<String> = env::args().collect();

  match &args[..] {
    [_] => run_repl(),
    [_, filepath] => run_file(filepath),
    _ => {
      eprintln!("Usage: rox [path]");
      exit(64)
    },
  }

  exit(0);
}

fn run_repl() {
  let mut vm = VM::init();
  let mut input = String::with_capacity(1024);
  let stdin = stdin();
  let mut stdout = stdout();
  loop {
    print!("> ");
    stdout.flush().unwrap();
    input.clear();
    match stdin.read_line(&mut input) {
      Err(error) => {
        println!("{error}");
        exit(65);
      },
      Ok(0) => {
        // Ctrl+d
        exit(0);
      },
      Ok(_) => {
        vm.interpret_partial(input.clone());
      },
    }
  }
}

fn run_file(filepath: &str) {
  match read_to_string(filepath) {
    Err(_) => {
      eprintln!("Could not read file \"{filepath}\".");
      exit(74);
    },
    Ok(source) => {
      let result = VM::interpret(source);

      match result {
        CompilationError => exit(65),
        RuntimeError => exit(70),
        Success => {},
      }
    },
  }
}
