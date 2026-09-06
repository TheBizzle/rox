#[derive(Debug)]
pub enum LexerError {
  UnexpectedToken,
  UnterminatedString,
}
