use std::env;
use std::fs;
use std::fs::read_to_string;
use std::io::{Write, stdin, stdout};
use std::path::Path;
use std::process::exit;

use rox_lib::vm::VM;
use rox_lib::vm::interpretation::Interpretation::{CompilationError, RuntimeError, Success};

#[tokio::main]
async fn main() {
  let args: Vec<String> = env::args().collect();

  match &args[..] {
    [_] => run_repl(),
    [_, filepath] => run_file(filepath),
    [_, flag, filepath] if flag == "--save" => save_file(Path::new(filepath)),
    [_, flag, filepath] if flag == "--load" => load_file(Path::new(filepath)),
    [_, flag, filepath] if flag == "--roundtrip" => roundtrip_file(Path::new(filepath)),
    _ => {
      eprintln!("Usage: rox [<path>|--save <path>|--load <path>|--roundtrip <path>]");
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
    Ok(source) => match VM::interpret(source).0 {
      CompilationError => exit(65),
      RuntimeError => exit(70),
      Success => {},
    },
  }
}

fn save_file(filepath: &Path) {
  match read_to_string(filepath) {
    Err(_) => {
      eprintln!("Could not read file \"{}\".", filepath.display());
      exit(74);
    },
    Ok(source) => {
      let result = VM::serialize(source).expect("Cannot save bytecode for file that does not compile");
      let new_path = filepath.with_extension("lbc");
      fs::write(new_path, result).unwrap();
    },
  }
}

fn load_file(filepath: &Path) {
  match read_to_string(filepath) {
    Err(_) => {
      eprintln!("Could not read file \"{}\".", filepath.display());
      exit(74);
    },
    Ok(source) => match VM::load_and_run(&source).0 {
      CompilationError => exit(65),
      RuntimeError => exit(70),
      Success => {},
    },
  }
}

fn roundtrip_file(filepath: &Path) {
  match read_to_string(filepath) {
    Err(_) => {
      eprintln!("Could not read file \"{}\".", filepath.display());
      exit(74);
    },
    Ok(source) => {
      if let Ok(serialized) = VM::serialize(source) {
        match VM::load_and_run(&serialized).0 {
          CompilationError => exit(65),
          RuntimeError => exit(70),
          Success => {},
        }
      } else {
        exit(65);
      }
    },
  }
}
