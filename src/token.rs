#[derive(Clone, Debug)]
pub struct Token {
  pub loc: SourceLoc,
  pub typ: TokenType,
}

#[derive(Clone, Debug)]
#[allow(unused)]
pub struct SourceLoc {
  pub start_index: u32,
  pub line_num: u32,
  pub column: u32,
  pub length: u32,
}

impl SourceLoc {
  pub fn extract(&self, source: &str) -> String {
    source.chars().skip(self.start_index as usize).take(self.length as usize).collect::<String>()
  }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenType {
  And,
  Bang,
  BangEqual,
  Class,
  Comma,
  Dot,
  Else,
  Eof,
  Equal,
  EqualEqual,
  False,
  For,
  Fun,
  Greater,
  GreaterEqual,
  Identifier(String),
  If,
  LeftBrace,
  LeftParen,
  Less,
  LessEqual,
  LoxString(String),
  Minus,
  Nil,
  Number(f64),
  Or,
  Plus,
  Print,
  Return,
  RightBrace,
  RightParen,
  Semicolon,
  Slash,
  Star,
  Super,
  This,
  True,
  Var,
  While,
}
