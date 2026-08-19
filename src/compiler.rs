use strum::FromRepr;

use crate::chunk::Chunk;

use crate::disassembler::disassemble_chunk;

use crate::opcode::OpCode::{Add, Constant, Divide, Multiply, Negate, Return as ReturnCode, Subtract};

use crate::parser::Parser;

use crate::token::Token;
use crate::token::TokenType::{self, Bang, Eof, LeftParen, Minus, Number, Plus, RightParen, Slash, Star};

use crate::value::Value::{self, Double};

const IS_DEBUGGING: bool = true;

#[derive(FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum Precedence {
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

type ParseFn<'a, 'b> = fn(&mut Compiler<'a, 'b>);

struct ParseRule<'a, 'b> {
  prefix: Option<ParseFn<'a, 'b>>,
  infix: Option<ParseFn<'a, 'b>>,
  precedence: Precedence,
}

fn rule_for<'a, 'b>(typ: &TokenType) -> ParseRule<'a, 'b> {
  let (prefix, infix, precedence): (Option<ParseFn<'a, 'b>>, Option<ParseFn<'a, 'b>>, Precedence) = match typ
  {
    LeftParen => (Some(Compiler::parse_grouping), None, Precedence::Bupkis),
    Minus => (Some(Compiler::parse_unary), Some(Compiler::parse_binary), Precedence::Term),
    Plus => (None, Some(Compiler::parse_binary), Precedence::Term),
    Slash | Star => (None, Some(Compiler::parse_binary), Precedence::Factor),
    Number(_) => (Some(Compiler::parse_number), None, Precedence::Bupkis),
    _ => (None, None, Precedence::Bupkis),
  };

  ParseRule { prefix, infix, precedence }
}

struct Compiler<'a, 'b> {
  parser: Parser<'a>,
  compiling_chunk: &'b mut Chunk,
}

pub fn compile(source: &str, chunk: &mut Chunk) -> bool {
  let mut compiler = Compiler::new(source, chunk);

  compiler.parser.advance();
  compiler.parse_expression();
  compiler.parser.consume(&Eof, "Expect end of expression.");
  compiler.end();
  !compiler.parser.had_error
}

impl<'a, 'b> Compiler<'a, 'b> {
  pub fn new(source: &'a str, chunk: &'b mut Chunk) -> Self {
    Compiler { parser: Parser::new(source), compiling_chunk: chunk }
  }

  fn end(&mut self) {
    self.emit_return();
    if IS_DEBUGGING && self.parser.had_error {
      disassemble_chunk(self.compiling_chunk, "code");
    }
  }

  fn emit_byte<T: Into<u8>>(&mut self, byte: T) {
    self.compiling_chunk.write(byte, self.parser.previous_token_opt.as_ref().unwrap().loc.line_num);
  }

  fn emit_bytes<T: Into<u8>, U: Into<u8>>(&mut self, byte1: T, byte2: U) {
    self.emit_byte(byte1);
    self.emit_byte(byte2);
  }

  fn emit_constant(&mut self, value: Value) {
    let constant = self.make_constant(value);
    self.emit_bytes(Constant, constant);
  }

  fn emit_return(&mut self) {
    self.emit_byte(ReturnCode);
  }

  fn make_constant(&mut self, value: Value) -> u8 {
    self.compiling_chunk.add_constant(value)
  }

  fn parse_binary(&mut self) {
    let operator_type = self.parser.previous_token_opt.as_ref().unwrap().typ.clone();
    let rule = rule_for(&operator_type);
    self.parse_precedence(&rule.precedence.next());

    let opcode_opt = match operator_type {
      Plus => Some(Add),
      Minus => Some(Subtract),
      Star => Some(Multiply),
      Slash => Some(Divide),
      _ => None,
    };

    if let Some(opcode) = opcode_opt {
      self.emit_byte(opcode);
    }
  }

  pub fn parse_expression(&mut self) {
    self.parse_precedence(&Precedence::Assignment);
  }

  pub fn parse_grouping(&mut self) {
    self.parse_expression();
    self.parser.consume(&RightParen, "Expect ')' after expression.");
  }

  pub fn parse_number(&mut self) {
    if let Some(Token { typ: Number(x), .. }) = self.parser.previous_token_opt {
      self.emit_constant(Double(x));
    }
  }

  fn parse_precedence(&mut self, p: &Precedence) {
    self.parser.advance();
    if let Some(prefix_rule) = rule_for(&self.parser.previous_token_opt.as_ref().unwrap().typ).prefix {
      prefix_rule(self);

      while p <= &rule_for(&self.parser.current_token_opt.as_ref().unwrap().typ).precedence {
        self.parser.advance();
        if let Some(infix_rule) = rule_for(&self.parser.previous_token_opt.as_ref().unwrap().typ).infix {
          infix_rule(self);
        }
      }
    } else {
      self.parser.error("Expect expression.");
    }
  }

  pub fn parse_unary(&mut self) {
    let operator_type = self.parser.previous_token_opt.as_ref().unwrap().typ.clone();

    self.parse_precedence(&Precedence::Unary);

    #[allow(clippy::match_same_arms)]
    match operator_type {
      Minus => {
        self.emit_byte(Negate);
      },
      Bang => {},
      _ => {},
    }
  }
}
