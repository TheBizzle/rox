#![allow(unused)]

#[derive(Eq, PartialEq)]
enum StackValueType {
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
  TypeTag,
  Any,
}

pub struct ShadowStack {
  stack: Vec<StackValueType>,
}

impl ShadowStack {
  pub const fn new() -> Self {
    Self { stack: Vec::new() }
  }

  pub const fn is_empty(&self) -> bool {
    self.stack.is_empty()
  }

  fn peek(&self, n: usize) -> Option<&StackValueType> {
    self.stack.get(self.stack.len() - 1 - n)
  }

  pub fn peek_any(&self, n: usize) -> bool {
    self.peek(n) == Some(&StackValueType::TypeTag)
  }

  pub fn peek_boolean(&self, n: usize) -> bool {
    self.peek(n) == Some(&StackValueType::Boolean)
  }

  pub fn peek_nil(&self, n: usize) -> bool {
    self.peek(n) == Some(&StackValueType::Nil)
  }

  pub fn peek_number(&self, n: usize) -> bool {
    self.peek(n) == Some(&StackValueType::Number)
  }

  pub fn peek_reference(&self, n: usize) -> bool {
    todo!("References are not yet implemented");
  }

  #[allow(clippy::unused_self)]
  pub const fn peek_string(&self, _n: usize) -> bool {
    false
  }

  pub fn pop(&mut self) {
    self.stack.pop();
  }

  pub fn push_any(&mut self) {
    self.stack.push(StackValueType::TypeTag);
  }

  pub fn push_boolean(&mut self) {
    self.stack.push(StackValueType::Boolean);
  }

  pub fn push_bound_method(&mut self) {
    self.stack.push(StackValueType::BoundMethod);
  }

  pub fn push_class(&mut self) {
    self.stack.push(StackValueType::Class);
  }

  pub fn push_closure(&mut self) {
    self.stack.push(StackValueType::Closure);
  }

  pub fn push_native_fn(&mut self) {
    self.stack.push(StackValueType::NativeFn);
  }

  pub fn push_nil(&mut self) {
    self.stack.push(StackValueType::Nil);
  }

  pub fn push_number(&mut self) {
    self.stack.push(StackValueType::Number);
  }

  pub fn push_object(&mut self) {
    self.stack.push(StackValueType::Object);
  }

  pub fn push_raw(&mut self) {
    self.stack.push(StackValueType::Raw);
  }

  pub fn push_string(&mut self) {
    self.stack.push(StackValueType::String);
  }

  pub fn push_upvalue(&mut self) {
    self.stack.push(StackValueType::Upvalue);
  }
}
