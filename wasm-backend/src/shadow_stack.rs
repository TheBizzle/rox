#![allow(unused)]

enum LoxType {
  Boolean,
  BoundMethod,
  Class,
  Closure,
  NativeFn,
  Nil,
  Number,
  Object,
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

  pub fn push_string(&mut self) {
    self.stack.push(LoxType::String);
  }

  pub fn push_upvalue(&mut self) {
    self.stack.push(LoxType::Upvalue);
  }
}
