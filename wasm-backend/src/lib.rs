pub mod wasm_result;

mod shadow_stack;
mod wasm_compiler;

use wasmparser::validate;

use rox_lib::compiler::Compiler;

use crate::wasm_compiler::WasmCompiler;
use crate::wasm_result::WasmResult::{self, CompilationError, Success, ValidationError};

#[must_use]
pub fn generate_wasm(source: &str) -> WasmResult {
  if let Some((compilation, _)) = Compiler::default().run(source.to_string()) {
    let wasm = WasmCompiler::new().run(&compilation);

    match validate(&wasm) {
      Ok(_) => Success { wasm },

      Err(err) => {
        let mut config = wasmprinter::Config::new();
        config.print_offsets(true);

        let mut storage = String::new();
        let printed = match config.offsets_and_lines(&wasm, &mut storage) {
          Ok(offset_line_pairs) => offset_line_pairs
            .map(|(offset_opt, line)| {
              offset_opt.map_or_else(
                || format!("          {line}"),
                |offset_number| format!("{offset_number:#06x}  {line}"),
              )
            })
            .collect(),

          Err(print_error) => format!("Could not print Wasm: {print_error}"),
        };

        ValidationError { message: printed + "\n\n" + &err.to_string() }
      },
    }
  } else {
    CompilationError
  }
}
