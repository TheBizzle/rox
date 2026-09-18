pub mod compiler;
pub mod core;
pub mod vm;

mod parser;
mod runtime;

#[cfg(target_arch = "wasm32")]
mod wasm {
  use wasm_bindgen::prelude::*;

  use crate::core::output::Output::{self, StdErr, StdErrLn, StdOut, StdOutLn};
  use crate::vm::VM;
  use crate::vm::interpretation::Interpretation::{CompilationError, RuntimeError, Success};

  #[wasm_bindgen]
  pub struct LoxRunner {
    vm: VM,
  }

  #[wasm_bindgen]
  impl LoxRunner {
    #[wasm_bindgen(constructor)]
    pub fn new() -> LoxRunner {
      LoxRunner { vm: VM::init() }
    }

    #[wasm_bindgen]
    pub fn run(&mut self, source: &str) -> Result<Vec<String>, Vec<String>> {
      let (interpretation, output) = self.vm.interpret_partial(source.to_string());
      let rendered_output = render_output(output);
      let res = match interpretation {
        CompilationError | RuntimeError => Err(rendered_output),
        Success => Ok(rendered_output),
      };
      res
    }
  }

  #[wasm_bindgen(start)]
  pub fn start() {
    console_error_panic_hook::set_once();
  }

  fn render_output(outputs: Vec<Output>) -> Vec<String> {
    let mut buffer = Vec::new();
    let mut did_just_push_newline = true;

    for output in outputs {
      match output {
        StdErrLn(s) | StdOutLn(s) => {
          buffer.push(s);
          did_just_push_newline = true;
        },
        StdErr(s) | StdOut(s) => {
          if did_just_push_newline {
            buffer.push(s);
          } else if let Some(partial) = buffer.last_mut() {
            partial.push_str(&s);
          } else {
            panic!("Impossible that we pushed a partial but have no string");
          }
          did_just_push_newline = false;
        },
      }
    }

    buffer
  }
}
