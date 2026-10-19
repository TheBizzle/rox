use super::token::Token;

#[derive(Debug)]
pub struct Ast {
  pub statements: Vec<Statement>,
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug)]
pub enum Statement {
  Block {
    contents: Vec<Self>,
    token: Token,
  },
  Class {
    class_var: Variable,
    super_name_var_opt: Option<Variable>,
    methods: Vec<Function>,
    token: Token,
  },
  DeclareVar {
    new_var: Variable,
    initial: Expr,
    token: Token,
  },
  ExpressionStatement {
    expr: Expr,
    token: Token,
  },
  ForStatement {
    initializer_opt: Option<Box<Self>>,
    cond_opt: Option<Expr>,
    incrementer_opt: Option<Expr>,
    body: Box<Self>,
    body_end_token: Token,
    token: Token,
  },
  FunctionStatement {
    func: Function,
    token: Token,
  },
  IfElse {
    antecedent: Expr,
    consequent: Box<Self>,
    alternative_opt: Option<Box<Self>>,
    token: Token,
  },
  PrintStatement {
    expr: Expr,
    token: Token,
  },
  ReturnStatement {
    expr_opt: Option<Expr>,
    token: Token,
  },
  WhileStatement {
    predicate: Expr,
    body: Box<Self>,
    body_end_token: Token,
    token: Token,
  },
}

#[derive(Clone, Debug)]
pub struct Function {
  pub fn_decl: Variable,
  pub params: Vec<Variable>,
  pub fn_body: Vec<Statement>,
  pub token: Token,
}

#[derive(Clone, Debug)]
pub struct Variable {
  pub name: String,
  pub token: Token,
}

#[allow(clippy::enum_variant_names, dead_code)]
#[derive(Clone, Debug)]
pub enum Literal {
  BooleanLit(bool),
  DoubleLit(f64),
  StringLit(String),
  NilLit,
}

#[allow(clippy::enum_variant_names, dead_code)]
#[derive(Clone, Debug)]
pub enum Expr {
  Assign { var: Variable, value: Box<Self>, token: Token },
  Binary { left: Box<Self>, right: Box<Self>, token: Token },
  Call { callee: Box<Self>, arguments: Vec<Self>, token: Token },
  Get { object: Box<Self>, var: Variable, token: Token },
  Grouping { expression: Box<Self>, token: Token },
  LiteralExpr { literal: Literal, token: Token },
  Logical { left: Box<Self>, right: Box<Self>, token: Token },
  Set { object: Box<Self>, var: Variable, value: Box<Self>, token: Token },
  Super { var: Variable, token: Token },
  This { token: Token },
  Unary { right: Box<Self>, token: Token },
  VarRef { var: Variable, token: Token },
}
