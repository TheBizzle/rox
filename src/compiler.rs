use crate::runtime::chunk::Chunk;

use crate::parser::Parser;

use crate::runtime::gc_object::GcObject;
use crate::runtime::heap::Heap;
use crate::runtime::heap_object::FunctionObj::{self, MainScript, UserDefined};
use crate::runtime::heap_object::HeapObject::HeapString;

use crate::parser::token::Token;
use crate::parser::token::TokenType::{
  self, Bang, BangEqual, Class, Comma, Dot, Else, Eof, Equal, EqualEqual, False, For, Fun, Greater,
  GreaterEqual, Identifier, If, LeftBrace, LeftParen, Less, LessEqual, Minus, Nil, Number, Plus, Print,
  Return, RightBrace, RightParen, Semicolon, Slash, Star, True, Var, While,
};

use crate::runtime::value::Value::{self, Double, Reference};

pub mod disassembler;
pub mod function_kind;
pub mod opcode;
pub mod program;

mod precedence;

use disassembler::disassemble_chunk;

use opcode::OpCode::{
  self, Add, Class as ClassCode, Closure, Constant, DefineGlobal, Divide, Equal as EqualCode,
  False as FalseCode, FnCall, GetGlobal, GetLocal, GetProperty, GetSuper, GetUpvalue, Greater as GreaterCode,
  Inherit, Invoke, Jump, JumpIfFalse, Less as LessCode, Loop, Method as MethodCode, Multiply, Negate,
  Nil as NilCode, Not, Pop, Print as PrintCode, Return as ReturnCode, SetGlobal, SetLocal, SetProperty,
  SetUpvalue, Subtract, SuperInvoke, True as TrueCode,
};

use precedence::{Precedence, rule_for};

use function_kind::FunctionKind::{self, Function, Initializer, Method, Script};
use program::{LocalVar::LocalBinding, Program, Upvalue};

const IS_DEBUGGING: bool = false;

struct JumpTarget(usize);

struct ClassContext {
  has_superclass: bool,
}

pub struct Compiler {
  class_contexts: Vec<ClassContext>,
  parser: Parser,
  programs: Vec<Program>,
  pub heap: Heap,
}

impl Compiler {
  fn program(&mut self) -> &mut Program {
    self.programs.last_mut().unwrap()
  }

  fn program_at(&mut self, index: usize) -> &mut Program {
    &mut self.programs[index]
  }
}

impl Default for Compiler {
  fn default() -> Self {
    Self {
      class_contexts: Vec::new(),
      parser: Parser::new(String::new()),
      programs: Vec::new(),
      heap: Heap::new(),
    }
  }
}

impl Compiler {
  pub fn mark_roots(&mut self) {
    for program in self.programs.iter().rev() {
      let function_gc = unsafe { &mut *program.function_gc_ptr };
      self.heap.mark_object(function_gc);
    }
  }

  pub fn run(&mut self, source: String) -> Option<(*mut FunctionObj, *mut GcObject)> {
    let script = MainScript { arity: 0, chunk: Chunk::default(), upvalue_count: 0 };
    self.programs = vec![Program::new(self.heap.allocate_function(script), Script)];

    self.parser = Parser::new(source);
    self.parser.advance();

    while !self.token_is_a(&Eof) {
      self.parse_declaration();
    }

    let result_ptr = self.end();
    (!self.parser.had_error).then_some(result_ptr)
  }

  fn end(&mut self) -> (*mut FunctionObj, *mut GcObject) {
    self.emit_return();
    if IS_DEBUGGING && self.parser.had_error {
      let fn_display = match self.program().function() {
        MainScript { .. } => "<script>".to_string(),
        UserDefined { name_gc_ptr, .. } => {
          let HeapString(name_ptr) = unsafe { &**name_gc_ptr }.object else {
            panic!("Not possible for name pointer to be non-string");
          };
          unsafe { &*name_ptr }.to_text()
        },
      };
      disassemble_chunk(self.program().chunk(), &fn_display);
    }
    (self.program().function(), self.program().function_gc_ptr)
  }

  fn emit_byte<T: Into<u8>>(&mut self, byte: T) {
    let line_num = self.parser.previous_token_opt.as_ref().unwrap().loc.line_num;
    self.program().chunk().write(byte, line_num);
  }

  fn emit_bytes<T: Into<u8>, U: Into<u8>>(&mut self, byte1: T, byte2: U) {
    self.emit_byte(byte1);
    self.emit_byte(byte2);
  }

  fn emit_constant(&mut self, value: Value) {
    let constant = self.make_constant(value);
    self.emit_bytes(Constant, constant);
  }

  fn emit_jump<T: Into<u8>>(&mut self, instruction: T) -> JumpTarget {
    self.emit_byte(instruction);
    self.emit_byte(0xff);
    self.emit_byte(0xff);
    JumpTarget(self.program().chunk().count - 2)
  }

  fn emit_loop(&mut self, jump_target: &JumpTarget) {
    let JumpTarget(loop_start) = jump_target;

    self.emit_byte(Loop);

    let offset = self.program().chunk().count - loop_start + 2;

    if offset > (u16::MAX as usize) {
      self.parser.error("Loop body too large.");
    }

    self.emit_byte(u8::try_from((offset >> 8) & 0xff).unwrap());
    self.emit_byte(u8::try_from(offset & 0xff).unwrap());
  }

  fn emit_return(&mut self) {
    if self.program().function_kind == Initializer {
      self.emit_bytes(GetLocal, 0);
    } else {
      self.emit_byte(NilCode);
    }

    self.emit_byte(ReturnCode);
  }

  fn make_constant(&mut self, value: Value) -> u8 {
    let chunk = self.program().chunk();
    if chunk.constants.count > u16::from(u8::MAX) {
      self.parser.error("Too many constants in one chunk.");
      0
    } else {
      chunk.add_constant(value)
    }
  }

  fn parse_and(&mut self, _can_assign: bool) {
    let end_jt = self.emit_jump(JumpIfFalse);

    self.emit_byte(Pop);
    self.parse_precedence(&Precedence::And);

    self.fill_in_jump_target(&end_jt);
  }

  fn parse_args(&mut self) -> u8 {
    let mut arg_count = 0;

    if self.parser.current_token_opt.as_ref().unwrap().typ != RightParen {
      loop {
        self.parse_expression();
        if arg_count == 255 {
          self.parser.error("Can't have more than 255 arguments.");
        }
        arg_count += 1;
        if !self.token_is_a(&Comma) {
          break;
        }
      }
    }

    self.parser.consume(&RightParen, "Expect ')' after arguments.");

    arg_count
  }

  // TODO: Most/all of these functions can probably move into the parser
  fn parse_binary(&mut self, _can_assign: bool) {
    enum Bytes {
      Zero,
      One(OpCode),
      Two(OpCode, OpCode),
    }
    use Bytes::{One, Two, Zero};

    let operator_type = self.parser.previous_token_opt.as_ref().unwrap().typ.clone();
    let rule = rule_for(&operator_type);
    self.parse_precedence(&rule.precedence.next());

    let opcodes = match operator_type {
      BangEqual => Two(EqualCode, Not),
      Bang => One(Not),
      EqualEqual => One(EqualCode),
      GreaterEqual => Two(LessCode, Not),
      Greater => One(GreaterCode),
      LessEqual => Two(GreaterCode, Not),
      Less => One(LessCode),
      Minus => One(Subtract),
      Plus => One(Add),
      Slash => One(Divide),
      Star => One(Multiply),
      _ => Zero,
    };

    match opcodes {
      Zero => {},
      One(opcode) => self.emit_byte(opcode),
      Two(op1, op2) => self.emit_bytes(op1, op2),
    }
  }

  fn parse_block(&mut self) {
    while !matches!(self.parser.current_token_opt.as_ref().unwrap().typ, RightBrace | Eof) {
      self.parse_declaration();
    }
    self.parser.consume(&RightBrace, "Expect '}' after block.");
  }

  fn parse_class_decl(&mut self) {
    if let Some(class_name) = self.parser.consume_dyn(
      |x| match x {
        Identifier(y) => Some(y.clone()),
        _ => None,
      },
      "Expect class name.",
    ) {
      let class_name_gc_ptr = {
        let loc = &self.parser.previous_token_opt.as_ref().unwrap().loc;
        self.heap.copy_string(&self.parser.source, loc).1
      };

      let name_byte = self.make_ident_constant();
      self.declare_variable(class_name.clone());

      self.emit_bytes(ClassCode, name_byte);
      self.define_variable(name_byte);
      self.class_contexts.push(ClassContext { has_superclass: false });

      if self.token_is_a(&Less) {
        if let Some(superclass_name) = self.parser.consume_dyn(
          |x| match x {
            Identifier(y) => Some(y.clone()),
            _ => None,
          },
          "Expect superclass name.",
        ) {
          self.make_named_variable(false); // Load superclass

          if class_name == superclass_name {
            self.parser.error("A class can't inherit from itself.");
          }

          self.program().begin_scope();
          self.add_local("super".to_string());
          self.define_variable(0);

          self.reference_named_variable(class_name_gc_ptr, false); // Load subclass

          self.emit_byte(Inherit);
          self.class_contexts.last_mut().unwrap().has_superclass = true;
        } else {
          while !matches!(self.parser.current_token_opt.as_ref().unwrap().typ, LeftBrace) {
            self.parser.advance();
          }
        }
      }

      self.reference_named_variable(class_name_gc_ptr, false);

      self.parser.consume(&LeftBrace, "Expect '{' before class body.");

      while !matches!(self.parser.current_token_opt.as_ref().unwrap().typ, RightBrace | Eof) {
        self.parse_method();
      }

      self.parser.consume(&RightBrace, "Expect '}' after class body.");

      self.emit_byte(Pop);

      if let Some(current_class) = self.class_contexts.last() {
        if current_class.has_superclass {
          for op_code in self.program().end_scope() {
            self.emit_byte(op_code);
          }
        }
        self.class_contexts.pop();
      }
    }
  }

  fn parse_declaration(&mut self) {
    if self.token_is_a(&Class) {
      self.parse_class_decl();
    } else if self.token_is_a(&Fun) {
      self.parse_function_decl();
    } else if self.token_is_a(&Var) {
      self.parse_var_decl();
    } else {
      self.parse_statement();
    }

    if self.parser.is_panicking {
      self.parser.synchronize();
    }
  }

  fn parse_dot(&mut self, can_assign: bool) {
    let property_opt = self.parser.consume_dyn(
      |x| match x {
        Identifier(y) => Some(y.clone()),
        _ => None,
      },
      "Expect property name after '.'.",
    );

    if property_opt.is_some() {
      let name_byte = self.make_ident_constant();

      if can_assign && self.token_is_a(&Equal) {
        self.parse_expression();
        self.emit_bytes(SetProperty, name_byte);
      } else if self.token_is_a(&LeftParen) {
        let arg_count = self.parse_args();
        self.emit_bytes(Invoke, name_byte);
        self.emit_byte(arg_count);
      } else {
        self.emit_bytes(GetProperty, name_byte);
      }
    }
  }

  fn parse_expression(&mut self) {
    self.parse_precedence(&Precedence::Assignment);
  }

  fn parse_expr_stmt(&mut self) {
    self.parse_expression();
    self.parser.consume(&Semicolon, "Expect ';' after expression.");
    self.emit_byte(Pop);
  }

  fn parse_for(&mut self) {
    self.program().begin_scope();

    self.parser.consume(&LeftParen, "Expect '(' after 'for'.");

    if self.token_is_a(&Var) {
      self.parse_var_decl();
    } else if !self.token_is_a(&Semicolon) {
      self.parse_expr_stmt();
    }

    let mut loop_start_jt = JumpTarget(self.program().chunk().count);

    let mut exit_jt_opt = None;
    if !self.token_is_a(&Semicolon) {
      self.parse_expression();
      self.parser.consume(&Semicolon, "Expect ';' after loop condition.");
      let _ = exit_jt_opt.insert(self.emit_jump(JumpIfFalse));
      self.emit_byte(Pop);
    }

    if !self.token_is_a(&RightParen) {
      let body_jt = self.emit_jump(Jump);
      let inc_start_jt = JumpTarget(self.program().chunk().count);

      self.parse_expression();
      self.emit_byte(Pop);
      self.parser.consume(&RightParen, "Expect ')' after for clauses.");

      self.emit_loop(&loop_start_jt);
      loop_start_jt = inc_start_jt;
      self.fill_in_jump_target(&body_jt);
    }

    self.parse_statement();
    self.emit_loop(&loop_start_jt);

    if let Some(exit_jt) = exit_jt_opt {
      self.fill_in_jump_target(&exit_jt);
      self.emit_byte(Pop); // Discards the condition --Jason B. (8/24/26)
    }

    for op_code in self.program().end_scope() {
      self.emit_byte(op_code);
    }
  }

  fn parse_function(&mut self, function_kind: FunctionKind) {
    let function_obj = {
      let prev_loc = &self.parser.previous_token_opt.as_ref().unwrap().loc;
      let (_, name_gc_ptr) = self.heap.copy_string(&self.parser.source, prev_loc);
      UserDefined { arity: 0, chunk: Chunk::default(), name_gc_ptr, upvalue_count: 0 }
    };

    self.programs.push(Program::new(self.heap.allocate_function(function_obj), function_kind));

    self.program().begin_scope();

    self.parser.consume(&LeftParen, "Expect '(' after function name.");

    if self.parser.current_token_opt.as_ref().unwrap().typ != RightParen {
      let mut arity = self.program().function().arity();

      loop {
        arity += 1;
        if arity > 255 {
          self.parser.error_at_current("Can't have more than 255 parameters.");
        }

        let constant = self.parse_variable("Expect parameter name.");
        self.define_variable(constant);

        if !self.token_is_a(&Comma) {
          break;
        }
      }

      self.program().function().set_arity(arity);
    }

    self.parser.consume(&RightParen, "Expect ')' after parameters.");
    self.parser.consume(&LeftBrace, "Expect '{' before function body.");
    self.parse_block();

    let upvalue_count = self.program().function().upvalue_count() as usize;
    let pairs: Vec<(u8, bool)> = self.program().upvalues[0..upvalue_count]
      .iter()
      .flatten()
      .map(|Upvalue { index, is_local }| (*index, *is_local))
      .collect();

    let (_, closure_gc_ptr) = self.end();
    let _ = self.programs.pop();

    let closure = self.make_constant(Reference(closure_gc_ptr));
    self.emit_bytes(Closure, closure);

    for (index, is_local) in pairs {
      self.emit_byte(u8::from(is_local));
      self.emit_byte(index);
    }
  }

  fn parse_function_call(&mut self, _can_assign: bool) {
    let arg_count = self.parse_args();
    self.emit_bytes(FnCall, arg_count);
  }

  fn parse_function_decl(&mut self) {
    let global_var_byte = self.parse_variable("Expect function name.");
    self.program().mark_latest_var_initialized();
    self.parse_function(Function);
    self.define_variable(global_var_byte);
  }

  fn parse_grouping(&mut self, _can_assign: bool) {
    self.parse_expression();
    self.parser.consume(&RightParen, "Expect ')' after expression.");
  }

  fn parse_if_else(&mut self) {
    self.parser.consume(&LeftParen, "Expect '(' after 'if'.");
    self.parse_expression();
    self.parser.consume(&RightParen, "Expect ')' after condition.");

    let consequent_jt = self.emit_jump(JumpIfFalse);
    self.emit_byte(Pop);
    self.parse_statement();
    let alternative_jt = self.emit_jump(Jump);
    self.fill_in_jump_target(&consequent_jt);

    self.emit_byte(Pop);
    if self.token_is_a(&Else) {
      self.parse_statement();
    }

    self.fill_in_jump_target(&alternative_jt);
  }

  fn parse_literal(&mut self, _can_assign: bool) {
    match self.parser.previous_token_opt.as_ref().unwrap().typ {
      False => self.emit_byte(FalseCode),
      Nil => self.emit_byte(NilCode),
      True => self.emit_byte(TrueCode),
      _ => {},
    }
  }

  fn parse_method(&mut self) {
    if let Some(name) = self.parser.consume_dyn(
      |x| match x {
        Identifier(y) => Some(y.clone()),
        _ => None,
      },
      "Expect method name.",
    ) {
      let function_kind = if name == "init" { Initializer } else { Method };

      let name_byte = self.make_ident_constant();
      self.parse_function(function_kind);
      self.emit_bytes(MethodCode, name_byte);
    }
  }

  fn parse_number(&mut self, _can_assign: bool) {
    if let Some(Token { typ: Number(x), .. }) = self.parser.previous_token_opt {
      self.emit_constant(Double(x));
    }
  }

  fn parse_or(&mut self, _can_assign: bool) {
    let else_jt = self.emit_jump(JumpIfFalse);
    let end_jt = self.emit_jump(Jump);
    self.fill_in_jump_target(&else_jt);

    self.emit_byte(Pop);
    self.parse_precedence(&Precedence::Or);

    self.fill_in_jump_target(&end_jt);
  }

  fn parse_precedence(&mut self, p: &Precedence) {
    self.parser.advance();
    if let Some(prefix_rule) = rule_for(&self.parser.previous_token_opt.as_ref().unwrap().typ).prefix {
      let can_assign = p <= &Precedence::Assignment;
      prefix_rule(self, can_assign);

      while p <= &rule_for(&self.parser.current_token_opt.as_ref().unwrap().typ).precedence {
        self.parser.advance();
        let Some(infix_rule) = rule_for(&self.parser.previous_token_opt.as_ref().unwrap().typ).infix else {
          panic!("Impossible missing infix rule");
        };
        infix_rule(self, can_assign);
      }

      if can_assign && self.token_is_a(&Equal) {
        self.parser.error("Invalid assignment target.");
      }
    } else {
      self.parser.error("Expect expression.");
    }
  }

  fn parse_return(&mut self) {
    if self.program().function_kind == Script {
      self.parser.error("Can't return from top-level code.");
    } else if self.token_is_a(&Semicolon) {
      self.emit_return();
    } else {
      if self.program().function_kind == Initializer {
        self.parser.error("Can't return a value from an initializer.");
      }
      self.parse_expression();
      self.parser.consume(&Semicolon, "Expect ';' after return value.");
      self.emit_byte(ReturnCode);
    }
  }

  fn parse_statement(&mut self) {
    if self.token_is_a(&Print) {
      self.parse_expression();
      self.parser.consume(&Semicolon, "Expect ';' after value.");
      self.emit_byte(PrintCode);
    } else if self.token_is_a(&If) {
      self.parse_if_else();
    } else if self.token_is_a(&For) {
      self.parse_for();
    } else if self.token_is_a(&Return) {
      self.parse_return();
    } else if self.token_is_a(&While) {
      self.parse_while();
    } else if self.token_is_a(&LeftBrace) {
      self.program().begin_scope();
      self.parse_block();
      for op_code in self.program().end_scope() {
        self.emit_byte(op_code);
      }
    } else {
      self.parse_expr_stmt();
    }
  }

  fn parse_string(&mut self, _can_assign: bool) {
    let mut fake_loc = self.parser.previous_token_opt.as_ref().unwrap().loc.clone();
    fake_loc.start_index += 1;
    fake_loc.length -= 2;
    let (_, gc_ptr) = self.heap.copy_string(&self.parser.source, &fake_loc);
    self.emit_constant(Reference(gc_ptr));
  }

  fn parse_super(&mut self, _can_assign: bool) {
    if self.class_contexts.is_empty() {
      self.parser.error("Can't use 'super' outside of a class.");
    } else if matches!(self.class_contexts.last(), Some(ClassContext { has_superclass: false })) {
      self.parser.error("Can't use 'super' in a class with no superclass.");
    }

    self.parser.consume(&Dot, "Expect '.' after 'super'.");

    let method_name_opt = self.parser.consume_dyn(
      |x| match x {
        Identifier(y) => Some(y.clone()),
        _ => None,
      },
      "Expect superclass method name.",
    );

    if method_name_opt.is_some() {
      let name_byte = self.make_ident_constant();
      self.reference_named_variable(self.heap.this_str_gc_ptr, false);

      if self.token_is_a(&LeftParen) {
        let arg_count = self.parse_args();
        self.reference_named_variable(self.heap.super_str_gc_ptr, false);
        self.emit_bytes(SuperInvoke, name_byte);
        self.emit_byte(arg_count);
      } else {
        self.reference_named_variable(self.heap.super_str_gc_ptr, false);
        self.emit_bytes(GetSuper, name_byte);
      }
    }
  }

  fn parse_this(&mut self, _can_assign: bool) {
    if self.class_contexts.is_empty() {
      self.parser.error("Can't use 'this' outside of a class.");
    } else {
      self.parse_var_reference(false);
    }
  }

  fn parse_unary(&mut self, _can_assign: bool) {
    let operator_type = self.parser.previous_token_opt.as_ref().unwrap().typ.clone();

    self.parse_precedence(&Precedence::Unary);

    #[allow(clippy::match_same_arms)]
    match operator_type {
      Minus => {
        self.emit_byte(Negate);
      },
      Bang => {
        self.emit_byte(Not);
      },
      _ => {},
    }
  }

  fn parse_var_decl(&mut self) {
    let global_var = self.parse_variable("Expect variable name.");

    if self.token_is_a(&Equal) {
      self.parse_expression();
    } else {
      self.emit_byte(NilCode);
    }
    self.parser.consume(&Semicolon, "Expect ';' after variable declaration.");

    self.define_variable(global_var);
  }

  fn parse_variable(&mut self, error_message: &str) -> u8 {
    let name_opt = self.parser.consume_dyn(
      |x| match x {
        Identifier(y) => Some(y.clone()),
        _ => None,
      },
      error_message,
    );

    #[allow(clippy::option_if_let_else)]
    if let Some(name) = name_opt {
      self.declare_variable(name);
      if self.program().scope_depth > 0 {
        0
      } else {
        self.make_ident_constant()
      }
    } else {
      0
    }
  }

  fn parse_var_reference(&mut self, can_assign: bool) {
    self.make_named_variable(can_assign);
  }

  fn parse_while(&mut self) {
    let loop_start_jt = JumpTarget(self.program().chunk().count);

    self.parser.consume(&LeftParen, "Expect '(' after 'while'.");
    self.parse_expression();
    self.parser.consume(&RightParen, "Expect ')' after condition.");

    let exit_jt = self.emit_jump(JumpIfFalse);
    self.emit_byte(Pop);
    self.parse_statement();
    self.emit_loop(&loop_start_jt);

    // The `pop` in this chunk is the same as the one above; it's simply that we need to pop the
    // condition, whether we continue into the `while` body or not. --Jason B. (8/24/26)
    self.fill_in_jump_target(&exit_jt);
    self.emit_byte(Pop);
  }

  fn add_local(&mut self, name: String) {
    if self.program().local_var_count > u16::from(u8::MAX) {
      self.parser.error("Too many local variables in function.");
    } else {
      let local = LocalBinding { name, depth_opt: None, is_captured: false };
      let index = self.program().local_var_count as usize;
      self.program().local_var_opts[index] = Some(local);
      self.program().local_var_count += 1;
    }
  }

  fn declare_variable(&mut self, new_var_name: String) {
    if self.program().scope_depth > 0 {
      for i in (0..self.program().local_var_count).rev() {
        let program = self.program();
        let scope_depth = program.scope_depth;
        if let Some(LocalBinding { name, depth_opt, .. }) = program.local_var_opts[i as usize].as_ref() {
          if let Some(depth) = depth_opt
            && depth < &scope_depth
          {
            break; // In this case, we're just shadowing, so no worries. --Jason B. (8/23/26)
          } else if &new_var_name == name {
            self.parser.error("Already a variable with this name in this scope.");
          }
        }
      }
      self.add_local(new_var_name);
    }
  }

  fn define_variable(&mut self, global_var: u8) {
    if self.program().scope_depth == 0 {
      self.emit_bytes(DefineGlobal, global_var);
    } else {
      self.program().mark_latest_var_initialized();
    }
  }

  fn fill_in_jump_target(&mut self, jump_target: &JumpTarget) {
    let JumpTarget(offset) = jump_target;
    let jt = self.program().chunk().count - offset - 2;

    if jt > (u16::MAX as usize) {
      self.parser.error("Too much code to jump over.");
    }

    unsafe {
      *self.program().chunk().op_codes.add(*offset) = u8::try_from((jt >> 8) & 0xff).unwrap();
      *self.program().chunk().op_codes.add(offset + 1) = u8::try_from(jt & 0xff).unwrap();
    }
  }

  fn make_ident_constant(&mut self) -> u8 {
    let loc = &self.parser.previous_token_opt.as_ref().unwrap().loc.clone();
    let (_, gc_ptr) = self.heap.copy_string(&self.parser.source, loc);
    self.reference_ident_constant(gc_ptr)
  }

  fn reference_ident_constant(&mut self, name_gc_ptr: *mut GcObject) -> u8 {
    #[allow(clippy::option_if_let_else)]
    if let Some(cached) = self.program().ident_byte_cache.get(&name_gc_ptr) {
      *cached
    } else {
      let generated = self.make_constant(Reference(name_gc_ptr));
      self.program().ident_byte_cache.insert(name_gc_ptr, generated);
      generated
    }
  }

  fn make_named_variable(&mut self, can_assign: bool) {
    let loc = &self.parser.previous_token_opt.as_ref().unwrap().loc.clone();
    let (_, gc_ptr) = self.heap.copy_string(&self.parser.source, loc);
    self.reference_named_variable(gc_ptr, can_assign);
  }

  fn reference_named_variable(&mut self, name_gc_ptr: *mut GcObject, can_assign: bool) {
    let HeapString(name_obj_ptr) = unsafe { &*name_gc_ptr }.object else {
      panic!("Token name must be a string");
    };
    let name = unsafe { &*name_obj_ptr }.to_text();
    let program_index = self.programs.len() - 1;
    let (arg, get_op, set_op) = if let Some(local_index) = self.resolve_local(program_index, &name) {
      (local_index, GetLocal, SetLocal)
    } else if let Some(x) = self.resolve_upvalue(self.programs.len() - 1, &name) {
      (x, GetUpvalue, SetUpvalue)
    } else {
      (self.reference_ident_constant(name_gc_ptr), GetGlobal, SetGlobal)
    };

    if can_assign && self.token_is_a(&Equal) {
      self.parse_expression();
      self.emit_bytes(set_op, arg);
    } else {
      self.emit_bytes(get_op, arg);
    }
  }

  fn register_upvalue(&mut self, pindex: usize, index: u8, is_local: bool) -> Option<u8> {
    let upvalue_count = self.program_at(pindex).function().upvalue_count();
    if upvalue_count > u16::from(u8::MAX) {
      self.parser.error("Too many closure variables in function.");
      None
    } else {
      let target_uv_opt = Some(Upvalue { index, is_local });
      let upvalue_index = upvalue_count as usize;
      let mut upvalues = self.program_at(pindex).upvalues[0..upvalue_index].iter();
      upvalues.position(|upvalue| &target_uv_opt == upvalue).map(|pos| u8::try_from(pos).unwrap()).or_else(
        || {
          self.program_at(pindex).upvalues[upvalue_index] = target_uv_opt;
          self.program_at(pindex).function().increment_upvalue_count();
          Some(u8::try_from(upvalue_count).unwrap())
        },
      )
    }
  }

  fn resolve_local(&mut self, program_index: usize, target_name: &str) -> Option<u8> {
    for i in (0..self.program_at(program_index).local_var_count).rev() {
      if let Some(LocalBinding { name, depth_opt, .. }) =
        self.program_at(program_index).local_var_opts[i as usize].as_ref()
        && target_name == name
      {
        if depth_opt.is_none() {
          self.parser.error("Can't read local variable in its own initializer.");
        } else {
          return Some(u8::try_from(i).unwrap());
        }
      }
    }
    None
  }

  fn resolve_upvalue(&mut self, program_index: usize, target_name: &str) -> Option<u8> {
    if self.program_at(program_index).function_kind == Script || program_index == 0 {
      None
    } else if let Some(local_index) = self.resolve_local(program_index - 1, target_name) {
      let program = self.program_at(program_index - 1);
      program.local_var_opts[local_index as usize].as_mut().unwrap().mark_captured();
      self.register_upvalue(program_index, local_index, true)
    } else if let Some(upvalue_index) = self.resolve_upvalue(program_index - 1, target_name) {
      self.register_upvalue(program_index, upvalue_index, false)
    } else {
      None
    }
  }

  /// # Panics
  /// When current token doesn't exist
  pub fn token_is_a(&mut self, typ: &TokenType) -> bool {
    if &self.parser.current_token_opt.as_ref().unwrap().typ != typ {
      return false;
    }
    self.parser.advance();
    true
  }
}
