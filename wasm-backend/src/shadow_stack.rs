#![allow(unused)]

#[derive(Eq, PartialEq)]
enum LoxType {
  Boolean,
  BoundMethod,
  Class,
  Closure,
  NativeFn,
  Nil,
  Number,
  Object,
  Raw,
  String,
  Upvalue,
}

pub struct ShadowStack {
  stack: Vec<LoxType>,
}

impl ShadowStack {
  pub const fn new() -> Self {
    Self { stack: Vec::new() }
  }

  pub const fn is_empty(&self) -> bool {
    self.stack.is_empty()
  }

  fn peek(&self, n: usize) -> Option<&LoxType> {
    self.stack.get(self.stack.len() - 1 - n)
  }

  pub fn peek_boolean(&self, n: usize) -> bool {
    self.peek(n) == Some(&LoxType::Boolean)
  }

  pub fn peek_nil(&self, n: usize) -> bool {
    self.peek(n) == Some(&LoxType::Nil)
  }

  pub fn peek_number(&self, n: usize) -> bool {
    self.peek(n) == Some(&LoxType::Number)
  }

  #[allow(clippy::unused_self)]
  pub const fn peek_string(&self, _n: usize) -> bool {
    false
  }

  pub fn pop(&mut self) {
    self.stack.pop();
  }

  pub fn push_boolean(&mut self) {
    self.stack.push(LoxType::Boolean);
  }

  pub fn push_bound_method(&mut self) {
    self.stack.push(LoxType::BoundMethod);
  }

  pub fn push_class(&mut self) {
    self.stack.push(LoxType::Class);
  }

  pub fn push_closure(&mut self) {
    self.stack.push(LoxType::Closure);
  }

  pub fn push_native_fn(&mut self) {
    self.stack.push(LoxType::NativeFn);
  }

  pub fn push_nil(&mut self) {
    self.stack.push(LoxType::Nil);
  }

  pub fn push_number(&mut self) {
    self.stack.push(LoxType::Number);
  }

  pub fn push_object(&mut self) {
    self.stack.push(LoxType::Object);
  }

  pub fn push_raw(&mut self) {
    self.stack.push(LoxType::Raw);
  }

  pub fn push_string(&mut self) {
    self.stack.push(LoxType::String);
  }

  pub fn push_upvalue(&mut self) {
    self.stack.push(LoxType::Upvalue);
  }
}
