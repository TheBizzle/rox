use core::str::Chars;

use std::iter::Peekable;

use crate::error::LexerError::{self, UnexpectedToken, UnterminatedString};
use crate::token::TokenType::{
  self, And, Bang, BangEqual, Class, Comma, Dot, Else, Eof, Equal, EqualEqual, False, For, Fun, Greater,
  GreaterEqual, Identifier, If, LeftBrace, LeftParen, Less, LessEqual, LoxString, Minus, Nil, Number, Or,
  Plus, Print, Return, RightBrace, RightParen, Semicolon, Slash, Star, Super, This, True, Var, While,
};
use crate::token::{SourceLoc, Token};

pub struct Lexer<'a> {
  chars: Peekable<Chars<'a>>,
  pos: u32,
  pos_prior: u32,
  last_newline_pos: u32,
  line_num: u32,
}

impl Lexer<'_> {
  pub fn new(source: &'_ str) -> Lexer<'_> {
    Lexer { chars: source.chars().peekable(), pos: 0, pos_prior: 0, last_newline_pos: 0, line_num: 1 }
  }

  pub fn next_token(&mut self) -> Result<Token, LexerError> {
    self.skip_trivia();

    match self.slurp_char_opt() {
      None => Ok(Eof),
      Some(c) if is_alphabetical(c) => Ok(self.make_namelike(c)),
      Some(c) if is_digit(c) => Ok(self.make_number(c)),
      Some('(') => Ok(LeftParen),
      Some(')') => Ok(RightParen),
      Some('{') => Ok(LeftBrace),
      Some('}') => Ok(RightBrace),
      Some(';') => Ok(Semicolon),
      Some(',') => Ok(Comma),
      Some('.') => Ok(Dot),
      Some('-') => Ok(Minus),
      Some('+') => Ok(Plus),
      Some('/') => Ok(Slash),
      Some('*') => Ok(Star),
      Some('!') => Ok(self.slurp_if_equals_char().map_or(Bang, |()| BangEqual)),
      Some('=') => Ok(self.slurp_if_equals_char().map_or(Equal, |()| EqualEqual)),
      Some('<') => Ok(self.slurp_if_equals_char().map_or(Less, |()| LessEqual)),
      Some('>') => Ok(self.slurp_if_equals_char().map_or(Greater, |()| GreaterEqual)),
      Some('"') => self.make_string(),
      _ => Err(UnexpectedToken),
    }
    .map(|tt| {
      let token = self.make_token(tt);
      self.pos_prior = self.pos;
      token
    })
  }

  const fn make_token(&self, typ: TokenType) -> Token {
    let start_index = self.pos_prior;
    let line_num = self.line_num;
    let column = self.pos_prior - self.last_newline_pos + 1;
    let length = self.pos - self.pos_prior;
    Token { typ, loc: SourceLoc { start_index, line_num, column, length } }
  }

  fn skip_trivia(&mut self) {
    loop {
      match self.chars.peek() {
        Some(' ' | '\r' | '\t') => {
          self.advance();
        },
        Some('\n') => {
          self.last_newline_pos = self.pos;
          self.line_num += 1;
          self.advance();
        },
        Some('/') => {
          let mut peekerator = self.chars.clone();
          let _ = peekerator.next();
          if peekerator.peek() == Some(&'/') {
            while !matches!(self.chars.peek(), Some('\n') | None) {
              self.advance();
            }
          } else {
            break;
          }
        },
        _ => {
          break;
        },
      }
    }
  }

  fn advance(&mut self) {
    let _ = self.slurp_char_opt();
    self.pos_prior = self.pos;
  }

  fn slurp_if_equals_char(&mut self) -> Option<()> {
    if self.chars.peek() == Some(&'=') {
      self.slurp_char_opt().map(|_| ())
    } else {
      None
    }
  }

  fn slurp_char_opt(&mut self) -> Option<char> {
    self.pos += 1;
    self.chars.next()
  }

  fn make_namelike(&mut self, c: char) -> TokenType {
    let mut namelike = c.to_string();
    while let Some(c) = self.chars.peek()
      && (is_digit(*c) || is_alphabetical(*c))
    {
      namelike.push(self.slurp_char_opt().unwrap());
    }

    match namelike.as_str() {
      "and" => And,
      "class" => Class,
      "else" => Else,
      "false" => False,
      "for" => For,
      "fun" => Fun,
      "if" => If,
      "nil" => Nil,
      "or" => Or,
      "print" => Print,
      "return" => Return,
      "super" => Super,
      "this" => This,
      "true" => True,
      "var" => Var,
      "while" => While,
      _ => Identifier(namelike),
    }
  }

  fn make_number(&mut self, c: char) -> TokenType {
    let mut num_str = c.to_string();
    while let Some(c) = self.chars.peek()
      && is_digit(*c)
    {
      num_str.push(self.slurp_char_opt().unwrap());
    }

    if self.chars.peek() == Some(&'.') {
      // Fractional time!
      let mut peekerator = self.chars.clone();
      let _ = peekerator.next();
      if let Some(c) = peekerator.peek()
        && is_digit(*c)
      {
        num_str.push(self.slurp_char_opt().unwrap()); // Push '.'
        while let Some(c) = self.chars.peek()
          && is_digit(*c)
        {
          num_str.push(self.slurp_char_opt().unwrap());
        }
      }
    }

    Number(num_str.parse::<f64>().unwrap())
  }

  fn make_string(&mut self) -> Result<TokenType, LexerError> {
    let mut str = String::new();
    while let c_opt = self.chars.peek()
      && c_opt != Some(&'"')
    {
      if c_opt.is_none() {
        return Err(UnterminatedString);
      } else if c_opt == Some(&'\n') {
        self.last_newline_pos = self.pos;
        self.line_num += 1;
      }
      str.push(self.slurp_char_opt().unwrap());
    }

    let _ = self.slurp_char_opt(); // Closing '"'
    Ok(LoxString(str))
  }
}

const fn is_alphabetical(c: char) -> bool {
  (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c == '_')
}

const fn is_digit(c: char) -> bool {
  matches!(c, '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9')
}
