use std::mem::take;

use crate::core::error::LexerError::{UnexpectedToken, UnterminatedString};
use crate::core::output::Output::{self, StdErr, StdErrLn, StdOut, StdOutLn};

mod lexer;
use lexer::Lexer;

pub mod token;
use token::{
  Token,
  TokenType::{self, Class, Eof, For, Fun, Identifier, If, Print, Return, Semicolon, Var, While},
};

pub struct Parser {
  lexer: Lexer,
  pub current_token_opt: Option<Token>,
  pub previous_token_opt: Option<Token>,
  pub had_error: bool,
  pub is_panicking: bool,
  pub source: String,
  wasm_output: Vec<Output>,
}

impl Parser {
  pub fn new(source: String) -> Self {
    let chars = source.chars().collect::<Vec<_>>().into_iter().peekable();
    Self {
      lexer: Lexer::new(chars),
      current_token_opt: None,
      previous_token_opt: None,
      had_error: false,
      is_panicking: false,
      source,
      wasm_output: Vec::new(),
    }
  }

  pub fn advance(&mut self) {
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

  pub fn consume(&mut self, typ: &TokenType, message: &str) {
    if &self.current_token_opt.as_ref().unwrap().typ == typ {
      self.advance();
    } else {
      self.error_at_current(message);
    }
  }

  pub fn consume_ident(&mut self, message: &str) -> Option<String> {
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

  pub fn synchronize(&mut self) {
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

  pub fn error_at_current(&mut self, message: &str) {
    self.error_at(self.current_token_opt.clone().as_ref(), message);
  }

  pub fn error(&mut self, message: &str) {
    self.error_at(self.previous_token_opt.clone().as_ref(), message);
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
}
