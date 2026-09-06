#[derive(Debug, Eq, PartialEq)]
pub enum FunctionKind {
  Function,
  Initializer,
  Method,
  Script,
}
