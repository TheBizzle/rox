use std::env;
use std::fs::{self, read_to_string};
use std::path::Path;
use std::process::exit;

use rox_wasm_backend::generate_wasm;
use rox_wasm_backend::wasm_result::WasmResult::{CompilationError, Success, ValidationError};

#[tokio::main]
async fn main() {
  let args: Vec<String> = env::args().collect();

  #[allow(clippy::single_match_else)]
  match &args[..] {
    [_, filepath] => run_file(Path::new(filepath)),
    _ => {
      eprintln!("Usage: rox-wasm-backend <path>");
      exit(64)
    },
  }

  exit(0);
}

fn run_file(filepath: &Path) {
  match read_to_string(filepath) {
    Err(_) => {
      eprintln!("Could not read file \"{}\".", filepath.display());
      exit(74);
    },
    Ok(source) => match generate_wasm(&source) {
      CompilationError => {
        eprintln!("Compilation error");
        exit(65)
      },
      ValidationError { message } => {
        eprintln!("Validation error: {message}");
        exit(70)
      },
      Success { wasm } => {
        let out_path = env::current_exe()
          .unwrap()
          .parent()
          .unwrap()
          .parent()
          .unwrap()
          .parent()
          .unwrap()
          .join("wasm-backend")
          .join("test-server")
          .join("program.wasm");
        fs::write(out_path.clone(), wasm).unwrap();
        println!("Successfully written to '{}'", out_path.display());
      },
    },
  }
}
