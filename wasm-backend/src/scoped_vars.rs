use std::collections::{HashMap, VecDeque};

use wasm_encoder::{ConstExpr, Ieee64};

use rox_lib::parser::ast::{
  Expr::{self, LiteralExpr, VarRef},
  Literal::{BooleanLit, DoubleLit, NilLit, StringLit},
  Statement::{
    self, Block, Class, DeclareVar, ExpressionStatement, ForStatement, FunctionStatement, IfElse,
    PrintStatement, ReturnStatement, WhileStatement,
  },
  Variable,
};

use super::lox_type::LoxType;
use super::wasm_const::{Boolean, WasmConst};

pub fn find_scope_vars(statements: &[Statement]) -> HashMap<(u8, String), (u32, ConstExpr, LoxType)> {
  let mut queue: VecDeque<_> = statements.iter().flat_map(|s| crawl_statement(s, 0)).collect();
  let mut map: HashMap<(u8, String), (u32, ConstExpr, LoxType)> = HashMap::new();

  while let Some((new_var, depth, initial)) = queue.pop_front() {
    let (value, typ) = match initial {
      LiteralExpr { literal, .. } => {
        let (wasm_const, typ) = match literal {
          NilLit => (WasmConst::NIL, LoxType::Nil),
          BooleanLit(false) => (WasmConst::I32(Boolean::False as i32), LoxType::Boolean),
          BooleanLit(true) => (WasmConst::I32(Boolean::True as i32), LoxType::Boolean),
          DoubleLit(x) => (WasmConst::F64(Ieee64::new(x.to_bits())), LoxType::Number),
          StringLit(_) => (WasmConst::I32(Boolean::True as i32), LoxType::Reference),
        };
        (wasm_const.as_v128(), typ)
      },

      VarRef { var, .. } if let Some((_, value, typ)) = resolve_var(&map, depth, &var.name) => {
        (value.clone(), typ.clone())
      },

      VarRef { .. } => {
        queue.push_back((new_var, depth, initial));
        continue;
      },

      x => {
        panic!("Not-yet-supported initial value: {x:?}");
      },
    };

    let index = u32::try_from(map.len()).unwrap();
    map.insert((depth, new_var.name), (index, value.clone(), typ));
  }

  map
}

#[allow(clippy::match_same_arms)]
fn crawl_statement(statement: &Statement, depth: u8) -> Vec<(Variable, u8, Expr)> {
  match statement {
    Block { contents, .. } => contents.iter().flat_map(|s| crawl_statement(s, depth + 1)).collect(),
    Class { .. } => Vec::new(), // TODO: Use `class_var`?
    DeclareVar { new_var, initial, .. } => vec![(new_var.clone(), depth, initial.clone())],
    ExpressionStatement { .. } => Vec::new(),
    ForStatement { initializer_opt, body, .. } => {
      let mut inits = initializer_opt.as_ref().map_or(Vec::new(), |s| crawl_statement(s.as_ref(), depth + 1));
      inits.extend(crawl_statement(body, depth + 1));
      inits
    },
    FunctionStatement { .. } => Vec::new(),
    IfElse { consequent, alternative_opt, .. } => {
      let alts = alternative_opt.as_ref().map_or(Vec::new(), |s| crawl_statement(s.as_ref(), depth));
      let mut conseq = crawl_statement(consequent, depth);
      conseq.extend(alts);
      conseq
    },
    PrintStatement { .. } => Vec::new(),
    ReturnStatement { .. } => Vec::new(),
    WhileStatement { body, .. } => crawl_statement(body, depth),
  }
}

pub fn extract_var<T>(map: &mut HashMap<(u8, String), T>, depth: u8, name: &str) -> Option<T> {
  for level in (0..=depth).rev() {
    if let Some(v) = map.remove(&(level, name.to_string())) {
      return Some(v);
    }
  }
  None
}

pub fn resolve_var<'a, T>(map: &'a HashMap<(u8, String), T>, depth: u8, name: &str) -> Option<&'a T> {
  for level in (0..=depth).rev() {
    if let Some(v) = map.get(&(level, name.to_string())) {
      return Some(v);
    }
  }
  None
}
