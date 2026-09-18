pub enum WasmResult {
  CompilationError,
  Success { wasm: Vec<u8> },
  ValidationError { message: String },
}
