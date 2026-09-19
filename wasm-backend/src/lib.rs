pub mod wasm_result;

mod shadow_stack;
mod wasm_compiler;

use wasmparser::validate;

use rox_lib::core::byte::Byte;

use rox_lib::compiler::Compiler;

use crate::wasm_compiler::WasmCompiler;
use crate::wasm_result::WasmResult::{self, CompilationError, Success, ValidationError};

#[must_use]
pub fn generate_wasm(source: &str) -> WasmResult {
  if let Some((compilation, _)) = Compiler::default().run(source.to_string()) {
    let bytecode: Vec<Byte> = compilation.main.line_data.values().flat_map(Clone::clone).collect();

    let wasm = WasmCompiler::new().run(&bytecode);

    match validate(&wasm) {
      Ok(_) => Success { wasm },
      Err(err) => ValidationError { message: err.to_string() },
    }
  } else {
    CompilationError
  }
}
