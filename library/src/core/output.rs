#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum Output {
  StdErr(String),
  StdErrLn(String),
  StdOut(String),
  StdOutLn(String),
}
