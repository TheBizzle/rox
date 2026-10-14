use crate::core::byte::Byte::{self, Named, Raw};

use crate::core::opcode::OpCode::{
  self, Add, Class as ClassCode, Closure, Constant, DefineGlobal, Divide, Equal as EqualCode,
  False as FalseCode, FnCall, GetGlobal, GetLocal, GetProperty, GetSuper, GetUpvalue, Greater as GreaterCode,
  Inherit, Invoke, Jump, JumpIfFalse, Less as LessCode, Loop, Method as MethodCode, Multiply, Negate,
  Nil as NilCode, Not, Pop, Print as PrintCode, Return as ReturnCode, SetGlobal, SetLocal, SetProperty,
  SetUpvalue, Subtract, SuperInvoke, True as TrueCode,
};

use crate::core::output::Output;

use crate::parser::Parser;
use crate::parser::ast::{Ast, Expr, Function as AstFunction, Literal, Statement, Variable};
use crate::parser::token::Token;
use crate::parser::token::TokenType::{
  And as AndToken, Bang, BangEqual, EqualEqual, Greater, GreaterEqual, Less, LessEqual, Minus, Plus, Slash,
  Star,
};

use crate::runtime::chunk::Chunk;
use crate::runtime::gc_object::{GcObject, GcPtr};
use crate::runtime::heap::Heap;
use crate::runtime::heap_object::FunctionObj::{self, MainScript, UserDefined};
use crate::runtime::heap_object::HeapObject::HeapString;
use crate::runtime::value::Value::{self, Double, Reference};

pub mod compilation;
pub mod disassembler;
pub mod function_kind;
pub mod program;

use compilation::{Compilation, crawl_root};
use disassembler::disassemble_chunk;
use function_kind::FunctionKind::{self, Function, Initializer, Method, Script};
use program::{LocalVar::LocalBinding, Program, Upvalue};

const IS_DEBUGGING: bool = false;

struct JumpTarget(usize);

pub struct Compiler {
  current_token_opt: Option<Token>,
  line_num: u32,
  parser: Parser,
  programs: Vec<Program>,
  pub heap: Heap,
}

impl Default for Compiler {
  fn default() -> Self {
    let mut heap = Heap::new();

    let script = MainScript { chunk: Chunk::default() };
    let programs = vec![Program::new(heap.allocate_function(script), Script)];

    Self { current_token_opt: None, line_num: 0, parser: Parser::new(String::new()), programs, heap }
  }
}

impl Compiler {
  pub fn get_root_fn_ptr(&mut self) -> *mut GcObject {
    self.program_at(0).function_gc_ptr
  }

  pub fn mark_roots(&mut self) {
    for program in self.programs.iter().rev() {
      let function_gc = unsafe { &mut *program.function_gc_ptr };
      self.heap.mark_object(function_gc);
    }
  }

  pub fn push_output(&mut self, output: Output) {
    self.parser.push_output(output);
  }

  pub fn take_wasm_output(&mut self) -> Vec<Output> {
    self.parser.take_wasm_output()
  }

  pub fn run(&mut self, source: String) -> Option<(Compilation, *mut GcObject)> {
    self.line_num = 0;
    let mut parser = Parser::new(source);
    let ast_opt = parser.parse();
    self.parser = parser;

    self.compile_ast(&ast_opt?);

    let (_, gc_ptr) = self.end();
    let compilation = crawl_root(gc_ptr);
    (!self.parser.had_error).then_some((compilation, gc_ptr))
  }

  fn compile_ast(&mut self, ast: &Ast) {
    for statement in &ast.statements {
      self.compile_declaration(statement);
    }
  }

  fn compile_declaration(&mut self, statement: &Statement) {
    self.compile_statement(statement);
    self.parser.is_panicking = false;
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

  fn set_token(&mut self, token: &Token) {
    self.line_num = self.line_num.max(token.loc.line_num);
    self.current_token_opt = Some(token.clone());
  }

  fn error(&mut self, message: &str) {
    if let Some(token) = self.current_token_opt.clone() {
      self.parser.previous_token_opt = Some(token);
    }
    self.parser.error(message);
  }

  fn emit_opcode(&mut self, opcode: OpCode) {
    self.emit_byte(Named(opcode));
  }

  fn emit_raw(&mut self, num: u8) {
    self.emit_byte(Raw(num));
  }

  fn emit_byte(&mut self, byte: Byte) {
    let line_num = self.line_num;
    self.program().chunk().write(byte, line_num);
  }

  fn emit_bytes(&mut self, opcode: OpCode, num: u8) {
    self.emit_opcode(opcode);
    self.emit_raw(num);
  }

  fn emit_constant(&mut self, value: Value) {
    let constant = self.make_constant(value);
    self.emit_bytes(Constant, constant);
  }

  fn emit_jump(&mut self, opcode: OpCode) -> JumpTarget {
    self.emit_opcode(opcode);
    self.emit_raw(0xff);
    self.emit_raw(0xff);
    JumpTarget(self.program().chunk().count - 2)
  }

  fn emit_loop(&mut self, jump_target: &JumpTarget) {
    let JumpTarget(loop_start) = jump_target;

    self.emit_opcode(Loop);

    let offset = self.program().chunk().count - loop_start + 2;

    if offset > (u16::MAX as usize) {
      self.error("Loop body too large.");
    }

    self.emit_raw(u8::try_from((offset >> 8) & 0xff).unwrap());
    self.emit_raw(u8::try_from(offset & 0xff).unwrap());
  }

  fn emit_return(&mut self) {
    if self.program().function_kind == Initializer {
      self.emit_bytes(GetLocal, 0);
    } else {
      self.emit_opcode(NilCode);
    }
    self.emit_opcode(ReturnCode);
  }

  fn end_scope(&mut self) {
    for op_code in self.program().end_scope() {
      self.emit_opcode(op_code);
    }
  }

  pub fn import_constants(&mut self, values: Vec<Value>) {
    for value in values {
      self.make_constant(value);
    }
  }

  pub fn import_byte(&mut self, byte: Byte, line_number: u32) {
    self.program().chunk().write(byte, line_number);
  }

  fn make_constant(&mut self, value: Value) -> u8 {
    let chunk = self.program().chunk();
    if chunk.constants.count > u16::from(u8::MAX) {
      self.error("Too many constants in one chunk.");
      0
    } else {
      chunk.add_constant(value)
    }
  }

  fn fill_in_jump_target(&mut self, jump_target: &JumpTarget) {
    let JumpTarget(offset) = jump_target;
    let jt = self.program().chunk().count - offset - 2;

    if jt > (u16::MAX as usize) {
      self.error("Too much code to jump over.");
    }

    unsafe {
      *self.program().chunk().op_codes.add(*offset) = Raw(u8::try_from((jt >> 8) & 0xff).unwrap());
      *self.program().chunk().op_codes.add(offset + 1) = Raw(u8::try_from(jt & 0xff).unwrap());
    }
  }

  #[allow(clippy::too_many_lines)]
  fn compile_statement(&mut self, statement: &Statement) {
    match statement {
      Statement::Block { contents, token } => {
        self.set_token(token);
        self.program().begin_scope();
        for inner in contents {
          self.compile_declaration(inner);
        }
        self.end_scope();
      },

      Statement::Class { class_var, super_name_var_opt, methods, token } => {
        self.set_token(token);
        self.compile_class(class_var, super_name_var_opt.as_ref(), methods);
      },

      Statement::DeclareVar { new_var, initial, token } => {
        self.set_token(token);
        let global_var = self.declare_named(new_var);
        self.compile_expr(initial);
        self.define_variable(global_var);
      },

      Statement::ExpressionStatement { expr, token } => {
        self.set_token(token);
        self.compile_expr(expr);
        self.emit_opcode(Pop);
      },

      Statement::ForStatement { initializer_opt, cond_opt, incrementer_opt, body, body_end_token, token } => {
        self.set_token(token);
        self.program().begin_scope();

        if let Some(initializer) = initializer_opt {
          self.compile_statement(initializer);
        }

        let mut loop_start_target = JumpTarget(self.program().chunk().count);

        let mut exit_target_opt = None;
        if let Some(cond) = cond_opt {
          self.compile_expr(cond);
          exit_target_opt = Some(self.emit_jump(JumpIfFalse));
          self.emit_opcode(Pop);
        }

        if let Some(incrementer) = incrementer_opt {
          let body_target = self.emit_jump(Jump);
          let inc_start_target = JumpTarget(self.program().chunk().count);

          self.compile_expr(incrementer);
          self.emit_opcode(Pop);

          self.emit_loop(&loop_start_target);
          loop_start_target = inc_start_target;
          self.fill_in_jump_target(&body_target);
        }

        self.compile_statement(body);
        self.set_token(body_end_token);
        self.emit_loop(&loop_start_target);

        if let Some(exit_target) = exit_target_opt {
          self.fill_in_jump_target(&exit_target);
          self.emit_opcode(Pop); // Discards the condition --Jason B. (8/24/26)
        }

        self.end_scope();
      },

      Statement::FunctionStatement { func, token } => {
        self.set_token(token);
        let global_var_byte = self.declare_named(&func.fn_decl);
        self.program().mark_latest_var_initialized();
        self.compile_function(func, Function);
        self.define_variable(global_var_byte);
      },

      Statement::IfElse { antecedent, consequent, alternative_opt, token } => {
        self.set_token(token);
        self.compile_expr(antecedent);

        let consequent_target = self.emit_jump(JumpIfFalse);
        self.emit_opcode(Pop);
        self.compile_statement(consequent);
        let alternative_target = self.emit_jump(Jump);
        self.fill_in_jump_target(&consequent_target);

        self.emit_opcode(Pop);
        if let Some(alternative) = alternative_opt {
          self.compile_statement(alternative);
        }

        self.fill_in_jump_target(&alternative_target);
      },

      Statement::PrintStatement { expr, token } => {
        self.set_token(token);
        self.compile_expr(expr);
        self.emit_opcode(PrintCode);
      },

      Statement::ReturnStatement { expr_opt, token } => {
        self.set_token(token);
        if let Some(expr) = expr_opt {
          self.compile_expr(expr);
          self.emit_opcode(ReturnCode);
        } else {
          self.emit_return();
        }
      },

      Statement::WhileStatement { predicate, body, body_end_token, token } => {
        self.set_token(token);
        let loop_start_target = JumpTarget(self.program().chunk().count);

        self.compile_expr(predicate);

        let exit_target = self.emit_jump(JumpIfFalse);
        self.emit_opcode(Pop);
        self.compile_statement(body);
        self.set_token(body_end_token);
        self.emit_loop(&loop_start_target);

        // The `pop` here is the same as the one above; it's simply that we need to pop the
        // condition, whether we continue into the `while` body or not. --Jason B. (8/24/26)
        self.fill_in_jump_target(&exit_target);
        self.emit_opcode(Pop);
      },
    }
  }

  fn compile_class(
    &mut self, class_var: &Variable, super_name_var_opt: Option<&Variable>, methods: &[AstFunction],
  ) {
    let name_byte = self.ident_constant(class_var);
    self.declare_variable(class_var);

    self.emit_bytes(ClassCode, name_byte);
    self.define_variable(name_byte);

    if let Some(super_var) = super_name_var_opt {
      self.load_variable(super_var);

      self.program().begin_scope();
      self.add_local("super".to_string());
      self.define_variable(0);

      self.load_variable(class_var);

      self.emit_opcode(Inherit);
    }

    self.load_variable(class_var);

    for method in methods {
      let name_byte = self.ident_constant(&method.fn_decl);
      let function_kind = if method.fn_decl.var_name == "init" {
        Initializer
      } else {
        Method
      };
      self.compile_function(method, function_kind);
      self.emit_bytes(MethodCode, name_byte);
    }

    self.emit_opcode(Pop);

    if super_name_var_opt.is_some() {
      self.end_scope();
    }
  }

  fn compile_function(&mut self, func: &AstFunction, function_kind: FunctionKind) {
    self.set_token(&func.token);

    let function_obj = {
      let (_, name_gc_ptr) = self.heap.copy_string(&self.parser.source, &func.fn_decl.var_token.loc);
      UserDefined { arity: 0, chunk: Chunk::default(), name_gc_ptr, upvalue_count: 0 }
    };

    self.programs.push(Program::new(self.heap.allocate_function(function_obj), function_kind));

    self.program().begin_scope();

    let mut arity = self.program().function().arity();
    for param in &func.params {
      arity += 1;
      self.declare_variable(param);
      self.define_variable(0);
    }
    self.program().function().set_arity(arity);

    for statement in &func.fn_body {
      self.compile_declaration(statement);
    }

    let upvalue_count = self.program().function().upvalue_count() as usize;
    let pairs: Vec<(u8, bool)> = self.program().upvalues[0..upvalue_count]
      .iter()
      .flatten()
      .map(|Upvalue { index, is_local }| (*index, *is_local))
      .collect();

    let (_, closure_gc_ptr) = self.end();
    let _ = self.programs.pop();

    let closure = self.make_constant(Reference(GcPtr(closure_gc_ptr)));
    self.emit_bytes(Closure, closure);

    for (index, is_local) in pairs {
      self.emit_raw(u8::from(is_local));
      self.emit_raw(index);
    }
  }

  #[allow(clippy::too_many_lines)]
  fn compile_expr(&mut self, expr: &Expr) {
    match expr {
      Expr::Assign { var, value, token } => {
        self.set_token(&var.var_token);
        let name_gc_ptr = self.name_ptr(&var.var_token);
        let (arg, _, set_op) = self.resolve_variable(&var.var_name, name_gc_ptr);
        self.compile_expr(value);
        self.set_token(token);
        self.emit_bytes(set_op, arg);
      },

      Expr::Binary { left, right, token } => {
        self.compile_expr(left);
        self.compile_expr(right);
        self.set_token(token);

        let opcodes = match token.typ {
          BangEqual => vec![EqualCode, Not],
          EqualEqual => vec![EqualCode],
          GreaterEqual => vec![LessCode, Not],
          Greater => vec![GreaterCode],
          LessEqual => vec![GreaterCode, Not],
          Less => vec![LessCode],
          Minus => vec![Subtract],
          Plus => vec![Add],
          Slash => vec![Divide],
          Star => vec![Multiply],
          _ => Vec::new(),
        };

        for opcode in opcodes {
          self.emit_opcode(opcode);
        }
      },

      Expr::Call { callee, arguments, token } => self.compile_call(callee, arguments, token),

      Expr::Get { object, var, token } => {
        self.compile_expr(object);
        let name_byte = self.ident_constant(var);
        self.set_token(token);
        self.emit_bytes(GetProperty, name_byte);
      },

      Expr::Grouping { expression, .. } => self.compile_expr(expression),

      Expr::LiteralExpr { literal, token } => {
        self.set_token(token);
        match literal {
          Literal::BooleanLit(true) => self.emit_opcode(TrueCode),
          Literal::BooleanLit(false) => self.emit_opcode(FalseCode),
          Literal::DoubleLit(x) => self.emit_constant(Double(*x)),
          Literal::NilLit => self.emit_opcode(NilCode),
          Literal::StringLit(_) => {
            let mut inner_loc = token.loc.clone();
            inner_loc.start_index += 1;
            inner_loc.length -= 2;
            let (_, gc_ptr) = self.heap.copy_string(&self.parser.source, &inner_loc);
            self.emit_constant(Reference(GcPtr(gc_ptr)));
          },
        }
      },

      Expr::Logical { left, right, token } => {
        self.compile_expr(left);
        self.set_token(token);
        if token.typ == AndToken {
          let end_target = self.emit_jump(JumpIfFalse);
          self.emit_opcode(Pop);
          self.compile_expr(right);
          self.fill_in_jump_target(&end_target);
        } else {
          let else_target = self.emit_jump(JumpIfFalse);
          let end_target = self.emit_jump(Jump);
          self.fill_in_jump_target(&else_target);
          self.emit_opcode(Pop);
          self.compile_expr(right);
          self.fill_in_jump_target(&end_target);
        }
      },

      Expr::Set { object, var, value, token } => {
        self.compile_expr(object);
        let name_byte = self.ident_constant(var);
        self.compile_expr(value);
        self.set_token(token);
        self.emit_bytes(SetProperty, name_byte);
      },

      Expr::Super { var, token } => {
        let name_byte = self.ident_constant(var);
        self.set_token(token);
        self.load_this();
        self.load_super();
        self.emit_bytes(GetSuper, name_byte);
      },

      Expr::This { token } => {
        self.set_token(token);
        let name_gc_ptr = self.name_ptr(token);
        let (arg, get_op, _) = self.resolve_variable("this", name_gc_ptr);
        self.emit_bytes(get_op, arg);
      },

      Expr::Unary { right, token } => {
        self.compile_expr(right);
        self.set_token(token);
        match token.typ {
          Minus => self.emit_opcode(Negate),
          Bang => self.emit_opcode(Not),
          _ => {},
        }
      },

      Expr::VarRef { var, .. } => self.load_variable(var),
    }
  }

  fn compile_call(&mut self, callee: &Expr, arguments: &[Expr], token: &Token) {
    match callee {
      Expr::Get { object, var, .. } => {
        self.compile_expr(object);
        let name_byte = self.ident_constant(var);
        let arg_count = self.compile_args(arguments);
        self.set_token(token);
        self.emit_bytes(Invoke, name_byte);
        self.emit_raw(arg_count);
      },

      Expr::Super { var, token: super_token } => {
        let name_byte = self.ident_constant(var);
        self.set_token(super_token);
        self.load_this();
        let arg_count = self.compile_args(arguments);
        self.set_token(token);
        self.load_super();
        self.emit_bytes(SuperInvoke, name_byte);
        self.emit_raw(arg_count);
      },

      _ => {
        self.compile_expr(callee);
        let arg_count = self.compile_args(arguments);
        self.set_token(token);
        self.emit_bytes(FnCall, arg_count);
      },
    }
  }

  fn compile_args(&mut self, arguments: &[Expr]) -> u8 {
    for argument in arguments {
      self.compile_expr(argument);
    }
    u8::try_from(arguments.len()).unwrap_or(u8::MAX)
  }

  fn add_local(&mut self, name: String) {
    if self.program().local_var_count > u16::from(u8::MAX) {
      self.error("Too many local variables in function.");
    } else {
      let local = LocalBinding { name, depth_opt: None, is_captured: false };
      let index = self.program().local_var_count as usize;
      self.program().local_var_opts[index] = Some(local);
      self.program().local_var_count += 1;
    }
  }

  fn declare_variable(&mut self, var: &Variable) {
    if self.program().scope_depth > 0 {
      self.set_token(&var.var_token);

      let mut is_duplicate = false;
      {
        let program = self.program();
        let scope_depth = program.scope_depth;
        for i in (0..program.local_var_count).rev() {
          if let Some(LocalBinding { name, depth_opt, .. }) = program.local_var_opts[i as usize].as_ref() {
            if let Some(depth) = depth_opt
              && *depth < scope_depth
            {
              break; // In this case, we're just shadowing, so no worries. --Jason B. (8/23/26)
            }
            if var.var_name == *name {
              is_duplicate = true;
            }
          }
        }
      }

      if is_duplicate {
        self.error("Already a variable with this name in this scope.");
      }

      self.add_local(var.var_name.clone());
    }
  }

  fn declare_named(&mut self, var: &Variable) -> u8 {
    self.declare_variable(var);
    if self.program().scope_depth > 0 {
      0
    } else {
      self.ident_constant(var)
    }
  }

  fn define_variable(&mut self, global_var: u8) {
    if self.program().scope_depth == 0 {
      self.emit_bytes(DefineGlobal, global_var);
    } else {
      self.program().mark_latest_var_initialized();
    }
  }

  fn name_ptr(&mut self, token: &Token) -> *mut GcObject {
    self.heap.copy_string(&self.parser.source, &token.loc).1
  }

  fn ident_constant(&mut self, var: &Variable) -> u8 {
    self.set_token(&var.var_token);
    let name_gc_ptr = self.name_ptr(&var.var_token);
    self.reference_ident_constant(name_gc_ptr)
  }

  fn reference_ident_constant(&mut self, name_gc_ptr: *mut GcObject) -> u8 {
    #[allow(clippy::option_if_let_else)]
    if let Some(cached) = self.program().ident_byte_cache.get(&name_gc_ptr) {
      *cached
    } else {
      let generated = self.make_constant(Reference(GcPtr(name_gc_ptr)));
      self.program().ident_byte_cache.insert(name_gc_ptr, generated);
      generated
    }
  }

  fn load_variable(&mut self, var: &Variable) {
    self.set_token(&var.var_token);
    let name_gc_ptr = self.name_ptr(&var.var_token);
    let (arg, get_op, _) = self.resolve_variable(&var.var_name, name_gc_ptr);
    self.emit_bytes(get_op, arg);
  }

  fn load_this(&mut self) {
    let name_gc_ptr = self.heap.this_str_gc_ptr;
    let (arg, get_op, _) = self.resolve_variable("this", name_gc_ptr);
    self.emit_bytes(get_op, arg);
  }

  fn load_super(&mut self) {
    let name_gc_ptr = self.heap.super_str_gc_ptr;
    let (arg, get_op, _) = self.resolve_variable("super", name_gc_ptr);
    self.emit_bytes(get_op, arg);
  }

  fn resolve_variable(&mut self, name: &str, name_gc_ptr: *mut GcObject) -> (u8, OpCode, OpCode) {
    let program_index = self.programs.len() - 1;

    #[allow(clippy::option_if_let_else)]
    if let Some(local_index) = self.resolve_local(program_index, name) {
      (local_index, GetLocal, SetLocal)
    } else if let Some(upvalue_index) = self.resolve_upvalue(program_index, name) {
      (upvalue_index, GetUpvalue, SetUpvalue)
    } else {
      (self.reference_ident_constant(name_gc_ptr), GetGlobal, SetGlobal)
    }
  }

  fn register_upvalue(&mut self, pindex: usize, index: u8, is_local: bool) -> Option<u8> {
    let upvalue_count = self.program_at(pindex).function().upvalue_count();
    if upvalue_count > u16::from(u8::MAX) {
      self.error("Too many closure variables in function.");
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
    let mut found = None;
    let mut read_in_own_initializer = false;

    for i in (0..self.program_at(program_index).local_var_count).rev() {
      if let Some(LocalBinding { name, depth_opt, .. }) =
        self.program_at(program_index).local_var_opts[i as usize].as_ref()
        && target_name == name
      {
        if depth_opt.is_none() {
          read_in_own_initializer = true;
        } else {
          found = Some(u8::try_from(i).unwrap());
          break;
        }
      }
    }

    if read_in_own_initializer {
      self.error("Can't read local variable in its own initializer.");
    }

    found
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

  fn program(&mut self) -> &mut Program {
    self.programs.last_mut().unwrap()
  }

  fn program_at(&mut self, index: usize) -> &mut Program {
    &mut self.programs[index]
  }
}
