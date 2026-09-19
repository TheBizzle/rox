pub mod wasm_result;

mod shadow_stack;
mod wasm_compiler;

use wasmparser::validate;

use rox_lib::compiler::Compiler;

use crate::wasm_compiler::WasmCompiler;
use crate::wasm_result::WasmResult::{self, CompilationError, Success, ValidationError};

#[must_use]
pub fn generate_wasm(source: &str) -> WasmResult {
  if let Some((_compilation, _)) = Compiler::default().run(source.to_string()) {
    // TODO: Crawl the `function_gc_ptr` like we're serializing the bytecode, and get all of the
    // strings and functions imported into the Wasm context
    let wasm = WasmCompiler::new().run(&Vec::new());

    match validate(&wasm) {
      Ok(_) => Success { wasm },
      Err(err) => ValidationError { message: err.to_string() },
    }
  } else {
    CompilationError
  }
}
