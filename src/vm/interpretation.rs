#[derive(Eq, PartialEq)]
pub enum Interpretation {
  CompilationError,
  RuntimeError,
  Success,
}
