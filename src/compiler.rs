use strum::FromRepr;

use crate::chunk::Chunk;

use crate::disassembler::disassemble_chunk;

use crate::opcode::OpCode::{
  self, Add, Constant, Divide, Equal as EqualCode, False as FalseCode, Greater as GreaterCode,
  Less as LessCode, Multiply, Negate, Nil as NilCode, Not, Return as ReturnCode, Subtract, True as TrueCode,
};

use crate::parser::Parser;

use crate::gc::Gc;

use crate::token::Token;
use crate::token::TokenType::{
  self, Bang, BangEqual, Eof, EqualEqual, False, Greater, GreaterEqual, LeftParen, Less, LessEqual,
  LoxString, Minus, Nil, Number, Plus, RightParen, Slash, Star, True,
};

use crate::value::Value::{self, Double, ReferenceValue};

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

type ParseFn<'a, 'b, 'c> = fn(&mut Compiler<'a, 'b, 'c>);

struct ParseRule<'a, 'b, 'c> {
  prefix: Option<ParseFn<'a, 'b, 'c>>,
  infix: Option<ParseFn<'a, 'b, 'c>>,
  precedence: Precedence,
}

fn rule_for<'a, 'b, 'c>(typ: &TokenType) -> ParseRule<'a, 'b, 'c> {
  let (prefix, infix, precedence): (Option<ParseFn<'a, 'b, 'c>>, Option<ParseFn<'a, 'b, 'c>>, Precedence) =
    match typ {
      Slash | Star => (None, Some(Compiler::parse_binary), Precedence::Factor),

      Minus => (Some(Compiler::parse_unary), Some(Compiler::parse_binary), Precedence::Term),

      Plus => (None, Some(Compiler::parse_binary), Precedence::Term),

      Greater | GreaterEqual | Less | LessEqual => {
        (None, Some(Compiler::parse_binary), Precedence::Comparison)
      },

      BangEqual | EqualEqual => (None, Some(Compiler::parse_binary), Precedence::Equality),

      Number(_) => (Some(Compiler::parse_number), None, Precedence::Bupkis),

      LoxString(_) => (Some(Compiler::parse_string), None, Precedence::Bupkis),

      Bang => (Some(Compiler::parse_unary), None, Precedence::Bupkis),

      LeftParen => (Some(Compiler::parse_grouping), None, Precedence::Bupkis),

      False | Nil | True => (Some(Compiler::parse_literal), None, Precedence::Bupkis),

      _ => (None, None, Precedence::Bupkis),
    };

  ParseRule { prefix, infix, precedence }
}

struct Compiler<'a, 'b, 'c> {
  parser: Parser<'a>,
  compiling_chunk: &'b mut Chunk,
  gc: &'c mut Gc,
}

pub fn compile(source: &str, chunk: &mut Chunk, gc: &mut Gc) -> bool {
  let mut compiler = Compiler::new(source, chunk, gc);

  compiler.parser.advance();
  compiler.parse_expression();
  compiler.parser.consume(&Eof, "Expect end of expression.");
  compiler.end();
  !compiler.parser.had_error
}

impl<'a, 'b, 'c> Compiler<'a, 'b, 'c> {
  pub fn new(source: &'a str, chunk: &'b mut Chunk, gc: &'c mut Gc) -> Self {
    Compiler { parser: Parser::new(source), compiling_chunk: chunk, gc }
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
    enum Bytes {
      Zero,
      One(OpCode),
      Two(OpCode, OpCode),
    }
    use Bytes::{One, Two, Zero};

    let operator_type = self.parser.previous_token_opt.as_ref().unwrap().typ.clone();
    let rule = rule_for(&operator_type);
    self.parse_precedence(&rule.precedence.next());

    let opcodes = match operator_type {
      BangEqual => Two(EqualCode, Not),
      Bang => One(Not),
      EqualEqual => One(EqualCode),
      GreaterEqual => Two(LessCode, Not),
      Greater => One(GreaterCode),
      LessEqual => Two(GreaterCode, Not),
      Less => One(LessCode),
      Minus => One(Subtract),
      Plus => One(Add),
      Slash => One(Divide),
      Star => One(Multiply),
      _ => Zero,
    };

    match opcodes {
      Zero => {},
      One(opcode) => self.emit_byte(opcode),
      Two(op1, op2) => self.emit_bytes(op1, op2),
    }
  }

  pub fn parse_expression(&mut self) {
    self.parse_precedence(&Precedence::Assignment);
  }

  pub fn parse_grouping(&mut self) {
    self.parse_expression();
    self.parser.consume(&RightParen, "Expect ')' after expression.");
  }

  pub fn parse_literal(&mut self) {
    match self.parser.previous_token_opt.as_ref().unwrap().typ {
      False => self.emit_byte(FalseCode),
      Nil => self.emit_byte(NilCode),
      True => self.emit_byte(TrueCode),
      _ => {},
    }
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

  pub fn parse_string(&mut self) {
    let prev_loc = &self.parser.previous_token_opt.as_ref().unwrap().loc;
    let str_start = (prev_loc.start_index + 1) as usize;
    let length = (prev_loc.length - 2) as usize;
    let str_ref = self.gc.copy_string(self.parser.source, str_start, length);
    self.emit_constant(ReferenceValue(str_ref));
  }

  pub fn parse_unary(&mut self) {
    let operator_type = self.parser.previous_token_opt.as_ref().unwrap().typ.clone();

    self.parse_precedence(&Precedence::Unary);

    #[allow(clippy::match_same_arms)]
    match operator_type {
      Minus => {
        self.emit_byte(Negate);
      },
      Bang => {
        self.emit_byte(Not);
      },
      _ => {},
    }
  }
}
