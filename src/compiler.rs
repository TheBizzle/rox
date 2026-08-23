use strum::FromRepr;

use crate::chunk::Chunk;

use crate::disassembler::disassemble_chunk;

use crate::opcode::OpCode::{
  self, Add, Constant, DefineGlobal, Divide, Equal as EqualCode, False as FalseCode, GetGlobal, GetLocal,
  Greater as GreaterCode, Less as LessCode, Multiply, Negate, Nil as NilCode, Not, Pop, Print as PrintCode,
  Return as ReturnCode, SetGlobal, SetLocal, Subtract, True as TrueCode,
};

use crate::parser::Parser;

use crate::gc::Gc;

use crate::token::Token;
use crate::token::TokenType::{
  self, Bang, BangEqual, Class, Eof, Equal, EqualEqual, False, For, Fun, Greater, GreaterEqual, Identifier,
  If, LeftBrace, LeftParen, Less, LessEqual, LoxString, Minus, Nil, Number, Plus, Print, Return, RightBrace,
  RightParen, Semicolon, Slash, Star, True, Var, While,
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

type ParseFn<'a, 'b, 'c> = fn(&mut Compiler<'a, 'b, 'c>, bool);

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

      Identifier(_) => (Some(Compiler::parse_var_reference), None, Precedence::Bupkis),

      _ => (None, None, Precedence::Bupkis),
    };

  ParseRule { prefix, infix, precedence }
}

#[derive(Debug)]
#[allow(unused)]
struct LocalVar {
  name: String,
  token: Token,
  depth_opt: Option<u8>,
}

struct Program {
  local_var_opts: Box<[Option<LocalVar>; u8::MAX as usize]>,
  local_var_count: u8,
  scope_depth: u8,
}

impl Program {
  #[must_use]
  pub fn new() -> Self {
    let size = u8::MAX as usize;
    let mut v = Vec::with_capacity(size);
    v.resize_with(size, || None);
    let local_var_opts = v.try_into().expect("Length must be exactly `u8::MAX`");

    Self { local_var_opts, local_var_count: 0, scope_depth: 0 }
  }

  pub const fn begin_scope(&mut self) {
    self.scope_depth += 1;
  }

  pub fn end_scope(&mut self) -> Vec<OpCode> {
    self.scope_depth -= 1;

    let mut op_codes = Vec::new();

    while self.local_var_count > 0
      && let index = (self.local_var_count - 1) as usize
      && let Some(depth) = self.local_var_opts[index].as_ref().unwrap().depth_opt
      && depth > self.scope_depth
    {
      op_codes.push(Pop);
      self.local_var_count -= 1;
    }

    op_codes
  }

  fn mark_latest_var_initialized(&mut self) {
    let index = (self.local_var_count - 1) as usize;
    let _ = self.local_var_opts[index].as_mut().unwrap().depth_opt.insert(self.scope_depth);
  }
}

struct Compiler<'a, 'b, 'c> {
  parser: Parser<'a>,
  program: Program,
  compiling_chunk: &'b mut Chunk,
  gc: &'c mut Gc,
}

pub fn compile(source: &str, chunk: &mut Chunk, gc: &mut Gc) -> bool {
  let mut compiler = Compiler::new(source, chunk, gc);

  compiler.parser.advance();

  while !compiler.token_is_a(&Eof) {
    compiler.parse_declaration();
  }

  compiler.end();
  !compiler.parser.had_error
}

impl<'a, 'b, 'c> Compiler<'a, 'b, 'c> {
  pub fn new(source: &'a str, chunk: &'b mut Chunk, gc: &'c mut Gc) -> Self {
    Compiler { parser: Parser::new(source), program: Program::new(), compiling_chunk: chunk, gc }
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

  fn parse_binary(&mut self, _can_assign: bool) {
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

  fn parse_block(&mut self) {
    while !matches!(self.parser.current_token_opt.as_ref().unwrap().typ, RightBrace | Eof) {
      self.parse_declaration();
    }
    self.parser.consume(&RightBrace, "Expect '}' after block.");
  }

  fn parse_declaration(&mut self) {
    if self.token_is_a(&Var) {
      self.parse_var_decl();
    } else {
      self.parse_statement();
    }

    if self.parser.is_panicking {
      self.synchronize();
    }
  }

  fn parse_expression(&mut self) {
    self.parse_precedence(&Precedence::Assignment);
  }

  fn parse_grouping(&mut self, _can_assign: bool) {
    self.parse_expression();
    self.parser.consume(&RightParen, "Expect ')' after expression.");
  }

  fn parse_literal(&mut self, _can_assign: bool) {
    match self.parser.previous_token_opt.as_ref().unwrap().typ {
      False => self.emit_byte(FalseCode),
      Nil => self.emit_byte(NilCode),
      True => self.emit_byte(TrueCode),
      _ => {},
    }
  }

  fn parse_number(&mut self, _can_assign: bool) {
    if let Some(Token { typ: Number(x), .. }) = self.parser.previous_token_opt {
      self.emit_constant(Double(x));
    }
  }

  fn parse_precedence(&mut self, p: &Precedence) {
    self.parser.advance();
    if let Some(prefix_rule) = rule_for(&self.parser.previous_token_opt.as_ref().unwrap().typ).prefix {
      let can_assign = p <= &Precedence::Assignment;
      prefix_rule(self, can_assign);

      while p <= &rule_for(&self.parser.current_token_opt.as_ref().unwrap().typ).precedence {
        self.parser.advance();
        if let Some(infix_rule) = rule_for(&self.parser.previous_token_opt.as_ref().unwrap().typ).infix {
          infix_rule(self, can_assign);
        }
        if can_assign && self.token_is_a(&Equal) {
          self.parser.error("Invalid assignment target.");
        }
      }
    } else {
      self.parser.error("Expect expression.");
    }
  }

  fn parse_statement(&mut self) {
    if self.token_is_a(&Print) {
      self.parse_expression();
      self.parser.consume(&Semicolon, "Expect ';' after value.");
      self.emit_byte(PrintCode);
    } else if self.token_is_a(&LeftBrace) {
      self.program.begin_scope();
      self.parse_block();
      for op_code in self.program.end_scope() {
        self.emit_byte(op_code);
      }
    } else {
      self.parse_expression();
      self.parser.consume(&Semicolon, "Expect ';' after expression.");
      self.emit_byte(Pop);
    }
  }

  fn parse_string(&mut self, _can_assign: bool) {
    let prev_loc = &self.parser.previous_token_opt.as_ref().unwrap().loc;
    let str_start = (prev_loc.start_index + 1) as usize;
    let length = (prev_loc.length - 2) as usize;
    let str_ref = self.gc.copy_string(self.parser.source, str_start, length);
    self.emit_constant(ReferenceValue(str_ref));
  }

  fn parse_unary(&mut self, _can_assign: bool) {
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

  fn parse_var_decl(&mut self) {
    let global_var = self.parse_variable("Expect variable name.");

    if self.token_is_a(&Equal) {
      self.parse_expression();
    } else {
      self.emit_byte(NilCode);
    }
    self.parser.consume(&Semicolon, "Expect ';' after variable declaration.");

    self.define_variable(global_var);
  }

  fn parse_variable(&mut self, error_message: &str) -> u8 {
    let name_opt = self.parser.consume_dyn(
      |x| match x {
        Identifier(y) => Some(y.clone()),
        _ => None,
      },
      error_message,
    );

    if let Some(name) = name_opt {
      self.declare_variable(name);
      if self.program.scope_depth > 0 {
        0
      } else {
        self.make_ident_constant()
      }
    } else {
      0
    }
  }

  fn parse_var_reference(&mut self, can_assign: bool) {
    self.make_named_variable(can_assign);
  }

  fn add_local(&mut self, name: String) {
    if self.program.local_var_count == u8::MAX {
      self.parser.error("Too many local variables in function.");
    } else {
      let token = self.parser.previous_token_opt.clone().unwrap();
      let local = LocalVar { name, token, depth_opt: None };
      let index = self.program.local_var_count as usize;
      self.program.local_var_opts[index] = Some(local);
      self.program.local_var_count += 1;
    }
  }

  fn declare_variable(&mut self, name: String) {
    if self.program.scope_depth > 0 {
      for i in (0..self.program.local_var_count).rev() {
        let local = self.program.local_var_opts[i as usize].as_ref().unwrap();
        if let Some(depth) = local.depth_opt
          && depth < self.program.scope_depth
        {
          break; // In this case, we're just shadowing, so no worries. --Jason B. (8/23/26)
        } else if name == local.name {
          self.parser.error("Already a variable with this name in this scope.");
        }
      }
      self.add_local(name);
    }
  }

  fn define_variable(&mut self, global_var: u8) {
    if self.program.scope_depth == 0 {
      self.emit_bytes(DefineGlobal, global_var);
    } else {
      self.program.mark_latest_var_initialized();
    }
  }

  fn make_ident_constant(&mut self) -> u8 {
    let loc = &self.parser.previous_token_opt.as_ref().unwrap().loc;
    let reference = self.gc.copy_string(self.parser.source, loc.start_index as usize, loc.length as usize);
    self.make_constant(ReferenceValue(reference))
  }

  fn make_named_variable(&mut self, can_assign: bool) {
    let loc = &self.parser.previous_token_opt.as_ref().unwrap().loc;
    let name = &self.parser.source[(loc.start_index as usize)..((loc.start_index + loc.length) as usize)];
    let (arg, get_op, set_op) = self.resolve_local(name).map_or_else(
      || (self.make_ident_constant(), GetGlobal, SetGlobal),
      |resolved| (resolved, GetLocal, SetLocal),
    );

    if can_assign && self.token_is_a(&Equal) {
      self.parse_expression();
      self.emit_bytes(set_op, arg);
    } else {
      self.emit_bytes(get_op, arg);
    }
  }

  fn resolve_local(&mut self, name: &str) -> Option<u8> {
    for i in (0..self.program.local_var_count).rev() {
      let local = self.program.local_var_opts[i as usize].as_ref().unwrap();
      if name == local.name {
        if local.depth_opt.is_none() {
          self.parser.error("Can't read local variable in its own initializer.");
        } else {
          return Some(i);
        }
      }
    }
    None
  }

  fn synchronize(&mut self) {
    self.parser.is_panicking = false;

    while self.parser.current_token_opt.as_ref().unwrap().typ != Eof
      && self.parser.previous_token_opt.as_ref().unwrap().typ != Semicolon
      && !matches!(
        &self.parser.current_token_opt.as_ref().unwrap().typ,
        Class | For | Fun | If | Print | Return | Var | While
      )
    {
      self.parser.advance();
    }
  }

  pub fn token_is_a(&mut self, typ: &TokenType) -> bool {
    if &self.parser.current_token_opt.as_ref().unwrap().typ != typ {
      return false;
    }
    self.parser.advance();
    true
  }
}
