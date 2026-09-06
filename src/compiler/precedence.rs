use strum::FromRepr;

use crate::parser::token::TokenType::{
  self, And, Bang, BangEqual, Dot, EqualEqual, False, Greater, GreaterEqual, Identifier, LeftParen, Less,
  LessEqual, LoxString, Minus, Nil, Number, Or, Plus, Slash, Star, Super, This, True,
};

use super::Compiler;

#[derive(FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub(super) enum Precedence {
  Bupkis,
  Assignment, // =
  Or,         // or
  And,        // and
  Equality,   // == !=
  Comparison, // < > <= >=
  Term,       // + -
  Factor,     // * /
  Unary,      // ! -
  Call,       // . ()
  Primary,
}

impl Precedence {
  pub const fn next(self) -> Self {
    Self::from_repr((self as u8) + 1).unwrap()
  }
}

pub(super) type ParseFn = fn(&mut Compiler, bool);

pub(super) struct ParseRule {
  pub(super) prefix: Option<ParseFn>,
  pub(super) infix: Option<ParseFn>,
  pub(super) precedence: Precedence,
}

pub(super) fn rule_for(typ: &TokenType) -> ParseRule {
  let (prefix, infix, precedence): (Option<ParseFn>, Option<ParseFn>, Precedence) = match typ {
    Dot => (None, Some(Compiler::parse_dot), Precedence::Call),

    LeftParen => (Some(Compiler::parse_grouping), Some(Compiler::parse_function_call), Precedence::Call),

    Slash | Star => (None, Some(Compiler::parse_binary), Precedence::Factor),

    Minus => (Some(Compiler::parse_unary), Some(Compiler::parse_binary), Precedence::Term),

    Plus => (None, Some(Compiler::parse_binary), Precedence::Term),

    Greater | GreaterEqual | Less | LessEqual => (None, Some(Compiler::parse_binary), Precedence::Comparison),

    BangEqual | EqualEqual => (None, Some(Compiler::parse_binary), Precedence::Equality),

    And => (None, Some(Compiler::parse_and), Precedence::And),

    Or => (None, Some(Compiler::parse_or), Precedence::Or),

    Number(_) => (Some(Compiler::parse_number), None, Precedence::Bupkis),

    LoxString(_) => (Some(Compiler::parse_string), None, Precedence::Bupkis),

    Bang => (Some(Compiler::parse_unary), None, Precedence::Bupkis),

    False | Nil | True => (Some(Compiler::parse_literal), None, Precedence::Bupkis),

    Identifier(_) => (Some(Compiler::parse_var_reference), None, Precedence::Bupkis),

    This => (Some(Compiler::parse_this), None, Precedence::Bupkis),

    Super => (Some(Compiler::parse_super), None, Precedence::Bupkis),

    _ => (None, None, Precedence::Bupkis),
  };

  ParseRule { prefix, infix, precedence }
}
