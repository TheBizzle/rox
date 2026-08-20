use crate::lexer::Lexer;

use crate::token::Token;
use crate::token::TokenType::{self, Eof};

pub struct Parser<'a> {
  lexer: Lexer<'a>,
  pub(super) current_token_opt: Option<Token>,
  pub(super) previous_token_opt: Option<Token>,
  pub(super) had_error: bool,
  is_panicking: bool,
  pub(super) source: &'a str,
}

impl Parser<'_> {
  pub fn new(source: &'_ str) -> Parser<'_> {
    Parser {
      lexer: Lexer::new(source),
      current_token_opt: None,
      previous_token_opt: None,
      had_error: false,
      is_panicking: false,
      source,
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
        Err(_error) => {
          // TODO: I don't think this is right...
          let message = self.current_token_opt.as_ref().unwrap().loc.extract(self.source);
          self.error_at_current(&message);
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

  pub fn error_at_current(&mut self, message: &str) {
    self.error_at(&self.current_token_opt.clone().unwrap(), message);
  }

  pub fn error(&mut self, message: &str) {
    self.error_at(&self.previous_token_opt.clone().unwrap(), message);
  }

  fn error_at(&mut self, token: &Token, message: &str) {
    if self.is_panicking {
      return;
    }

    let Token { loc, typ } = token;

    self.is_panicking = true;
    eprint!("[line {}] Error", loc.line_num);

    match typ {
      Eof => {
        eprint!(" at end");
      },
      // TODO: And handle (i.e. do nothing) when "error token" was "emitted"
      _ => {
        eprint!(" at '{}'", loc.extract(self.source));
      },
    }

    eprintln!(": {message}");
    self.had_error = true;
  }
}
