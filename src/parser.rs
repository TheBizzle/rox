use crate::core::error::LexerError::{UnexpectedToken, UnterminatedString};

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
      Some(std::mem::take(y))
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
    // TODO: Print out elsewhere
    eprint!("[line {line_num}] Error");

    if let Some(Token { loc, typ }) = token_opt {
      match typ {
        Eof => {
          eprint!(" at end");
        },
        _ => {
          eprint!(" at '{}'", loc.extract(&self.source));
        },
      }
    }

    eprintln!(": {message}");
    self.had_error = true;
  }
}
