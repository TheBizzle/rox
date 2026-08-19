use crate::lexer::Lexer;

use crate::token::{Token, TokenType::Eof};

pub fn compile(source: &str) {
  let mut lexer = Lexer::new(source);

  let mut line_num = 0;

  loop {
    match lexer.next_token() {
      Err(error) => {
        eprintln!("{error:?}");
        break;
      },
      Ok(Token { loc, typ }) => {
        if loc.line_num == line_num {
          print!("   | ");
        } else {
          print!("{:4} ", loc.line_num);
          line_num = loc.line_num;
        }
        let value = if typ == Eof {
          ""
        } else {
          let range = (loc.start_index as usize)..((loc.start_index + loc.length) as usize);
          &source.to_string()[range]
        };
        println!("{typ:2?} '{value}'");

        if typ == Eof {
          break;
        }
      },
    }
  }
}
