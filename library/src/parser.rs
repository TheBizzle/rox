use std::mem::take;

use crate::core::error::LexerError::{UnexpectedToken, UnterminatedString};
use crate::core::output::Output::{self, StdErr, StdErrLn, StdOut, StdOutLn};

use crate::compiler::function_kind::FunctionKind;

pub mod ast;
pub mod token;

mod lexer;
mod precedence;

use ast::{Ast, Expr, Function, Literal, Statement, Variable};
use lexer::Lexer;
use precedence::{Precedence, infix_precedence};
use token::Token;
use token::TokenType::{
  self, And as AndToken, Bang, BangEqual, Class, Comma, Dot, Else, Eof, Equal, EqualEqual, False, For, Fun,
  Greater, GreaterEqual, Identifier, If, LeftBrace, LeftParen, Less, LessEqual, LoxString, Minus, Nil,
  Number, Or as OrToken, Plus, Print, Return, RightBrace, RightParen, Semicolon, Slash, Star,
  Super as SuperToken, This as ThisToken, True, Var, While,
};

struct ClassContext {
  has_superclass: bool,
}

pub struct Parser {
  lexer: Lexer,
  class_contexts: Vec<ClassContext>,
  pub current_token_opt: Option<Token>,
  pub previous_token_opt: Option<Token>,
  function_kinds: Vec<FunctionKind>,
  pub had_error: bool,
  pub is_panicking: bool,
  pub source: String,
  wasm_output: Vec<Output>,
}

impl Parser {
  #[must_use]
  pub fn new(source: String) -> Self {
    let chars = source.chars().collect::<Vec<_>>().into_iter().peekable();
    Self {
      lexer: Lexer::new(chars),
      class_contexts: Vec::new(),
      current_token_opt: None,
      previous_token_opt: None,
      function_kinds: Vec::new(),
      had_error: false,
      is_panicking: false,
      source,
      wasm_output: Vec::new(),
    }
  }

  pub fn parse(&mut self) -> Option<Ast> {
    self.advance();

    let mut statements = Vec::new();
    while !self.token_is_a(&Eof) {
      if let Some(statement) = self.parse_declaration() {
        statements.push(statement);
      }
    }

    (!self.had_error).then_some(Ast { statements })
  }

  pub fn push_output(&mut self, output: Output) {
    if cfg!(target_arch = "wasm32") {
      self.wasm_output.push(output);
    } else {
      match output {
        StdErr(s) => eprint!("{s}"),
        StdErrLn(s) => eprintln!("{s}"),
        StdOut(s) => print!("{s}"),
        StdOutLn(s) => println!("{s}"),
      }
    }
  }

  pub fn take_wasm_output(&mut self) -> Vec<Output> {
    take(&mut self.wasm_output)
  }

  pub fn error(&mut self, message: &str) {
    self.error_at(self.previous_token_opt.clone().as_ref(), message);
  }

  fn error_at_current(&mut self, message: &str) {
    self.error_at(self.current_token_opt.clone().as_ref(), message);
  }

  fn error_at(&mut self, token_opt: Option<&Token>, message: &str) {
    if self.is_panicking {
      return;
    }

    self.is_panicking = true;

    let line_num = if let Some(Token { loc, .. }) = token_opt.or(self.previous_token_opt.as_ref()) {
      loc.line_num
    } else {
      self.lexer.get_line_num()
    };
    self.push_output(StdErr(format!("[line {line_num}] Error")));

    if let Some(Token { loc, typ }) = token_opt {
      match typ {
        Eof => {
          self.push_output(StdErr(" at end".to_string()));
        },
        _ => {
          self.push_output(StdErr(format!(" at '{}'", loc.extract(&self.source))));
        },
      }
    }

    self.push_output(StdErrLn(format!(": {message}")));
    self.had_error = true;
  }

  fn advance(&mut self) {
    self.previous_token_opt = self.current_token_opt.take();

    loop {
      match self.lexer.next_token() {
        Ok(token) => {
          self.current_token_opt = Some(token);
          break;
        },
        Err(error) => {
          let message = match error {
            UnexpectedToken => "Unexpected character.",
            UnterminatedString => "Unterminated string.",
          };
          self.error_at(None, message);
        },
      }
    }
  }

  fn consume(&mut self, typ: &TokenType, message: &str) {
    if &self.current_token_opt.as_ref().unwrap().typ == typ {
      self.advance();
    } else {
      self.error_at_current(message);
    }
  }

  fn consume_ident(&mut self, message: &str) -> Option<String> {
    let out_opt = if let Identifier(y) = &mut self.current_token_opt.as_mut().unwrap().typ {
      Some(take(y))
    } else {
      self.error_at_current(message);
      None
    };

    if out_opt.is_some() {
      self.advance();
    }

    out_opt
  }

  fn synchronize(&mut self) {
    self.is_panicking = false;

    while self.current_token_opt.as_ref().unwrap().typ != Eof
      && self.previous_token_opt.as_ref().unwrap().typ != Semicolon
      && !matches!(
        &self.current_token_opt.as_ref().unwrap().typ,
        Class | For | Fun | If | Print | Return | Var | While
      )
    {
      self.advance();
    }
  }

  fn previous(&self) -> Token {
    self.previous_token_opt.as_ref().unwrap().clone()
  }

  fn current(&self) -> Token {
    self.current_token_opt.as_ref().unwrap().clone()
  }

  const fn current_type(&self) -> &TokenType {
    &self.current_token_opt.as_ref().unwrap().typ
  }

  /// # Panics
  /// When current token doesn't exist
  fn token_is_a(&mut self, typ: &TokenType) -> bool {
    if self.current_type() == typ {
      self.advance();
      true
    } else {
      false
    }
  }

  fn lexeme(&self, token: &Token) -> String {
    let start = token.loc.start_index as usize;
    let end = start + token.loc.length as usize;
    self.source.get(start..end).unwrap_or_default().to_string()
  }

  fn string_contents(&self, token: &Token) -> String {
    let start = token.loc.start_index as usize;
    let end = start + token.loc.length as usize;
    if end < start + 2 {
      return String::new();
    }
    self.source.get(start + 1..end - 1).unwrap_or_default().to_string()
  }

  fn consume_variable(&mut self, error_message: &str) -> Option<Variable> {
    let name = self.consume_ident(error_message)?;
    Some(Variable { name, token: self.previous() })
  }

  const fn nil_expr(token: Token) -> Expr {
    Expr::LiteralExpr { literal: Literal::NilLit, token }
  }

  fn parse_declaration(&mut self) -> Option<Statement> {
    let statement = if self.token_is_a(&Class) {
      self.parse_class_decl()
    } else if self.token_is_a(&Fun) {
      self.parse_function_decl()
    } else if self.token_is_a(&Var) {
      self.parse_var_decl()
    } else {
      self.parse_statement()
    };

    if self.is_panicking {
      self.synchronize();
    }

    statement
  }

  fn parse_class_decl(&mut self) -> Option<Statement> {
    let token = self.previous();
    let class_var = self.consume_variable("Expect class name.")?;

    self.class_contexts.push(ClassContext { has_superclass: false });
    let statement = self.parse_class_rest(class_var, token);
    self.class_contexts.pop();

    Some(statement)
  }

  fn parse_class_rest(&mut self, class_var: Variable, token: Token) -> Statement {
    let mut super_name_var_opt = None;

    if self.token_is_a(&Less) {
      if let Some(super_var) = self.consume_variable("Expect superclass name.") {
        if super_var.name == class_var.name {
          self.error("A class can't inherit from itself.");
        }
        super_name_var_opt = Some(super_var);
        self.class_contexts.last_mut().unwrap().has_superclass = true;
      } else {
        while !matches!(self.current_type(), LeftBrace | Eof) {
          self.advance();
        }
      }
    }

    self.consume(&LeftBrace, "Expect '{' before class body.");

    let mut methods = Vec::new();
    while !matches!(self.current_type(), RightBrace | Eof) {
      if let Some(method) = self.parse_method() {
        methods.push(method);
      } else if !matches!(self.current_type(), RightBrace | Eof) {
        self.advance();
      }
    }

    self.consume(&RightBrace, "Expect '}' after class body.");

    Statement::Class { class_var, super_name_var_opt, methods, token }
  }

  fn parse_function_decl(&mut self) -> Option<Statement> {
    let token = self.previous();
    let name = self.consume_variable("Expect function name.")?;
    let func = self.parse_function(name, FunctionKind::Function);
    Some(Statement::FunctionStatement { func, token })
  }

  fn parse_method(&mut self) -> Option<Function> {
    let name = self.consume_variable("Expect method name.")?;
    let kind = if name.name == "init" {
      FunctionKind::Initializer
    } else {
      FunctionKind::Method
    };
    Some(self.parse_function(name, kind))
  }

  fn parse_function(&mut self, fn_decl: Variable, kind: FunctionKind) -> Function {
    let token = fn_decl.token.clone();
    self.function_kinds.push(kind);

    self.consume(&LeftParen, "Expect '(' after function name.");

    let mut params = Vec::new();
    if !matches!(self.current_type(), RightParen) {
      loop {
        if params.len() >= 255 {
          self.error_at_current("Can't have more than 255 parameters.");
        }

        if let Some(param) = self.consume_variable("Expect parameter name.") {
          params.push(param);
        }

        if !self.token_is_a(&Comma) {
          break;
        }
      }
    }

    self.consume(&RightParen, "Expect ')' after parameters.");
    self.consume(&LeftBrace, "Expect '{' before function body.");
    let fn_body = self.parse_block();

    self.function_kinds.pop();

    Function { fn_decl, params, fn_body, token }
  }

  fn parse_var_decl(&mut self) -> Option<Statement> {
    let token = self.previous();
    let new_var = self.consume_variable("Expect variable name.")?;

    let initial = if self.token_is_a(&Equal) {
      self.parse_expression()?
    } else {
      Self::nil_expr(new_var.token.clone())
    };

    self.consume(&Semicolon, "Expect ';' after variable declaration.");

    Some(Statement::DeclareVar { new_var, initial, token })
  }

  fn parse_statement(&mut self) -> Option<Statement> {
    if self.token_is_a(&Print) {
      self.parse_print()
    } else if self.token_is_a(&If) {
      self.parse_if_else()
    } else if self.token_is_a(&For) {
      self.parse_for()
    } else if self.token_is_a(&Return) {
      self.parse_return()
    } else if self.token_is_a(&While) {
      self.parse_while()
    } else if self.token_is_a(&LeftBrace) {
      let token = self.previous();
      let contents = self.parse_block();
      Some(Statement::Block { contents, token })
    } else {
      self.parse_expr_stmt()
    }
  }

  fn parse_block(&mut self) -> Vec<Statement> {
    let mut contents = Vec::new();

    while !matches!(self.current_type(), RightBrace | Eof) {
      if let Some(statement) = self.parse_declaration() {
        contents.push(statement);
      }
    }

    self.consume(&RightBrace, "Expect '}' after block.");

    contents
  }

  fn parse_expr_stmt(&mut self) -> Option<Statement> {
    let token = self.current();
    let expr = self.parse_expression()?;
    self.consume(&Semicolon, "Expect ';' after expression.");
    Some(Statement::ExpressionStatement { expr, token })
  }

  fn parse_print(&mut self) -> Option<Statement> {
    let token = self.previous();
    let expr = self.parse_expression()?;
    self.consume(&Semicolon, "Expect ';' after value.");
    Some(Statement::PrintStatement { expr, token })
  }

  fn parse_if_else(&mut self) -> Option<Statement> {
    let token = self.previous();

    self.consume(&LeftParen, "Expect '(' after 'if'.");
    let antecedent = self.parse_expression()?;
    self.consume(&RightParen, "Expect ')' after condition.");

    let consequent = Box::new(self.parse_statement()?);
    let alternative_opt = if self.token_is_a(&Else) {
      Some(Box::new(self.parse_statement()?))
    } else {
      None
    };

    Some(Statement::IfElse { antecedent, consequent, alternative_opt, token })
  }

  fn parse_return(&mut self) -> Option<Statement> {
    let token = self.previous();

    if self.function_kinds.is_empty() {
      self.error("Can't return from top-level code.");
      return None;
    }

    if self.token_is_a(&Semicolon) {
      return Some(Statement::ReturnStatement { expr_opt: None, token });
    }

    if self.function_kinds.last() == Some(&FunctionKind::Initializer) {
      self.error("Can't return a value from an initializer.");
    }

    let expr = self.parse_expression()?;
    self.consume(&Semicolon, "Expect ';' after return value.");

    Some(Statement::ReturnStatement { expr_opt: Some(expr), token })
  }

  fn parse_while(&mut self) -> Option<Statement> {
    let token = self.previous();

    self.consume(&LeftParen, "Expect '(' after 'while'.");
    let predicate = self.parse_expression()?;
    self.consume(&RightParen, "Expect ')' after condition.");

    let body = Box::new(self.parse_statement()?);
    let body_end_token = self.previous();

    Some(Statement::WhileStatement { predicate, body, body_end_token, token })
  }

  fn parse_for(&mut self) -> Option<Statement> {
    let token = self.previous();
    let mut did_fail = false;

    self.consume(&LeftParen, "Expect '(' after 'for'.");

    let initializer_opt = if self.token_is_a(&Semicolon) {
      None
    } else {
      let initializer = if self.token_is_a(&Var) {
        self.parse_var_decl()
      } else {
        self.parse_expr_stmt()
      };
      did_fail |= initializer.is_none();
      initializer.map(Box::new)
    };

    let cond_opt = if self.token_is_a(&Semicolon) {
      None
    } else {
      let cond = self.parse_expression();
      did_fail |= cond.is_none();
      self.consume(&Semicolon, "Expect ';' after loop condition.");
      cond
    };

    let incrementer_opt = if self.token_is_a(&RightParen) {
      None
    } else {
      let incrementer = self.parse_expression();
      did_fail |= incrementer.is_none();
      self.consume(&RightParen, "Expect ')' after for clauses.");
      incrementer
    };

    let body_opt = self.parse_statement();
    let body_end_token = self.previous();

    if !did_fail && let Some(body) = body_opt {
      Some(Statement::ForStatement {
        initializer_opt,
        cond_opt,
        incrementer_opt,
        body: Box::new(body),
        body_end_token,
        token,
      })
    } else {
      None
    }
  }

  fn parse_expression(&mut self) -> Option<Expr> {
    self.parse_precedence(Precedence::Assignment)
  }

  fn parse_precedence(&mut self, p: Precedence) -> Option<Expr> {
    self.advance();

    let can_assign = p <= Precedence::Assignment;
    let mut expr = self.parse_prefix(can_assign)?;

    while p <= infix_precedence(self.current_type()) {
      self.advance();
      expr = self.parse_infix(expr, can_assign)?;
    }

    if can_assign && self.token_is_a(&Equal) {
      self.error("Invalid assignment target.");
    }

    Some(expr)
  }

  fn parse_prefix(&mut self, can_assign: bool) -> Option<Expr> {
    let token = self.previous();

    match token.typ.clone() {
      Nil => Some(Expr::LiteralExpr { literal: Literal::NilLit, token }),
      True => Some(Expr::LiteralExpr { literal: Literal::BooleanLit(true), token }),
      False => Some(Expr::LiteralExpr { literal: Literal::BooleanLit(false), token }),
      Number(x) => Some(Expr::LiteralExpr { literal: Literal::DoubleLit(x), token }),

      LoxString { .. } => {
        Some(Expr::LiteralExpr { literal: Literal::StringLit(self.string_contents(&token)), token })
      },

      Identifier { .. } => {
        let var = Variable { name: self.lexeme(&token), token: token.clone() };
        if can_assign && self.token_is_a(&Equal) {
          let eq_token = self.previous();
          let value = Box::new(self.parse_expression()?);
          Some(Expr::Assign { var, value, token: eq_token })
        } else {
          Some(Expr::VarRef { var, token })
        }
      },

      Bang | Minus => {
        let right = Box::new(self.parse_precedence(Precedence::Unary)?);
        Some(Expr::Unary { right, token })
      },

      LeftParen => {
        let expression = Box::new(self.parse_expression()?);
        self.consume(&RightParen, "Expect ')' after expression.");
        Some(Expr::Grouping { expression, token })
      },

      SuperToken => {
        if self.class_contexts.is_empty() {
          self.error("Can't use 'super' outside of a class.");
        } else if matches!(self.class_contexts.last(), Some(ClassContext { has_superclass: false })) {
          self.error("Can't use 'super' in a class with no superclass.");
        }

        self.consume(&Dot, "Expect '.' after 'super'.");
        let var = self.consume_variable("Expect superclass method name.")?;
        Some(Expr::Super { var, token })
      },

      ThisToken => {
        if self.class_contexts.is_empty() {
          self.error("Can't use 'this' outside of a class.");
        }
        Some(Expr::This { token })
      },

      _ => {
        self.error("Expect expression.");
        None
      },
    }
  }

  fn parse_infix(&mut self, left: Expr, can_assign: bool) -> Option<Expr> {
    let token = self.previous();

    match token.typ.clone() {
      AndToken => {
        let right = self.parse_precedence(Precedence::And)?;
        Some(Expr::Logical { left: Box::new(left), right: Box::new(right), token })
      },

      Dot => {
        let var = self.consume_variable("Expect property name after '.'.")?;
        if can_assign && self.token_is_a(&Equal) {
          let value = Box::new(self.parse_expression()?);
          Some(Expr::Set { object: Box::new(left), var, value, token })
        } else {
          Some(Expr::Get { object: Box::new(left), var, token })
        }
      },

      LeftParen => {
        let arguments = self.parse_args()?;
        Some(Expr::Call { callee: Box::new(left), arguments, token })
      },

      OrToken => {
        let right = self.parse_precedence(Precedence::Or)?;
        Some(Expr::Logical { left: Box::new(left), right: Box::new(right), token })
      },

      BangEqual | EqualEqual | Greater | GreaterEqual | Less | LessEqual | Minus | Plus | Slash | Star => {
        let precedence = infix_precedence(&token.typ);
        let right = self.parse_precedence(precedence.next())?;
        Some(Expr::Binary { left: Box::new(left), right: Box::new(right), token })
      },

      _ => panic!("Impossible missing infix rule"),
    }
  }

  fn parse_args(&mut self) -> Option<Vec<Expr>> {
    let mut arguments = Vec::new();

    if !matches!(self.current_type(), RightParen) {
      loop {
        arguments.push(self.parse_expression()?);

        if arguments.len() > 255 {
          self.error("Can't have more than 255 arguments.");
        }

        if !self.token_is_a(&Comma) {
          break;
        }
      }
    }

    self.consume(&RightParen, "Expect ')' after arguments.");

    Some(arguments)
  }
}
