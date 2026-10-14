use super::token::TokenType::{
  self, And as AndToken, BangEqual, Dot, EqualEqual, Greater, GreaterEqual, LeftParen, Less, LessEqual,
  Minus, Or as OrToken, Plus, Slash, Star,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precedence {
  None,
  Assignment,
  Or,
  And,
  Equality,
  Comparison,
  Term,
  Factor,
  Unary,
  Call,
  Primary,
}

impl Precedence {
  #[must_use]
  pub const fn next(self) -> Self {
    match self {
      Self::None | Self::Assignment => Self::Or,
      Self::Or => Self::And,
      Self::And => Self::Equality,
      Self::Equality => Self::Comparison,
      Self::Comparison => Self::Term,
      Self::Term => Self::Factor,
      Self::Factor => Self::Unary,
      Self::Unary => Self::Call,
      Self::Call | Self::Primary => Self::Primary,
    }
  }
}

#[must_use]
pub const fn infix_precedence(typ: &TokenType) -> Precedence {
  match typ {
    LeftParen | Dot => Precedence::Call,
    Minus | Plus => Precedence::Term,
    Slash | Star => Precedence::Factor,
    BangEqual | EqualEqual => Precedence::Equality,
    Greater | GreaterEqual | Less | LessEqual => Precedence::Comparison,
    AndToken => Precedence::And,
    OrToken => Precedence::Or,
    _ => Precedence::None,
  }
}
