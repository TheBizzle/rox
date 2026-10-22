use std::collections::HashMap;

use strum::{EnumCount, FromRepr, IntoEnumIterator};

use wasm_encoder::{
  BlockType, CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function,
  FunctionSection, GlobalSection, GlobalType, Ieee64, ImportSection, MemorySection, MemoryType, Module,
  TypeSection, ValType,
};

use rox_lib::parser::token::Token;
use rox_lib::parser::token::TokenType::{
  And, Bang, BangEqual, EqualEqual, Greater, GreaterEqual, Less, LessEqual, Minus, Or, Plus, Slash, Star,
};

use rox_lib::parser::ast::{
  Ast,
  Expr::{self, Assign, Binary, Call, Get, Grouping, LiteralExpr, Logical, Set, Super, This, Unary, VarRef},
  Function as AstFunction,
  Literal::{self, BooleanLit, DoubleLit, NilLit, StringLit},
  Statement::{
    self, Block, Class, DeclareVar, ExpressionStatement, ForStatement, FunctionStatement, IfElse,
    PrintStatement, ReturnStatement, WhileStatement,
  },
  Variable,
};

use super::lox_type::LoxType;
use super::scoped_vars::{extract_var, find_scope_vars, resolve_var};
use super::shadow_stack::ShadowStack;
use super::wasm_const::{Boolean, NIL, WasmConst};

pub struct WasmCompiler {
  stack: ShadowStack,
}

struct Context<'a> {
  compiler: &'a mut WasmCompiler,
  depth: u8,
  clock_fn_index: u32,
  error_fn_index: u32,
  function: Function,
  globals_map: HashMap<(u8, String), (u32, ConstExpr, LoxType)>,
  logical_type_index: u32,
  print_number_fn_index: u32,
  print_fn_index: u32,
  print_int_fn_index: u32,
}

impl Context<'_> {
  pub fn push_type(&mut self) {
    let typ = if self.compiler.stack.peek_nil(0) {
      LoxType::Nil
    } else if self.compiler.stack.peek_boolean(0) {
      LoxType::Boolean
    } else if self.compiler.stack.peek_number(0) {
      LoxType::Number
    } else {
      panic!("Unsupported stack item type");
    };
    self.function.instructions().i32_const(typ as i32);
    self.compiler.stack.push_raw();
  }

  pub fn push_bool(&mut self, bool: bool) {
    let value = if bool { Boolean::True } else { Boolean::False };
    self.function.instructions().i32_const(value as i32);
    self.compiler.stack.push_boolean();
  }

  pub fn push_nil(&mut self) {
    self.function.instructions().i32_const(NIL);
    self.compiler.stack.push_nil();
  }

  pub fn push_number(&mut self, num: f64) {
    self.function.instructions().f64_const(Ieee64::new(num.to_bits()));
    self.compiler.stack.push_number();
  }

  pub fn push_boolean_as_any(&mut self) {
    self.function.instructions().i64_extend_i32_u().i32_const(LoxType::Boolean as i32);
    self.compiler.stack.pop();
    self.compiler.stack.push_any();
  }

  pub fn push_nil_as_any(&mut self) {
    self.function.instructions().i64_extend_i32_u().i32_const(LoxType::Nil as i32);
    self.compiler.stack.pop();
    self.compiler.stack.push_any();
  }

  pub fn push_number_as_any(&mut self) {
    self.function.instructions().i64_reinterpret_f64().i32_const(LoxType::Number as i32);
    self.compiler.stack.pop();
    self.compiler.stack.push_any();
  }

  pub fn push_boolean_as_v128(&mut self) {
    self
      .function
      .instructions()
      .i32x4_splat()
      .i32_const(LoxType::Boolean as i32)
      .i32x4_replace_lane(0)
      .i32_const(0)
      .i32x4_replace_lane(1)
      .i32_const(0)
      .i32x4_replace_lane(3);
  }

  pub fn push_nil_as_v128(&mut self) {
    self
      .function
      .instructions()
      .i32x4_splat()
      .i32_const(LoxType::Nil as i32)
      .i32x4_replace_lane(0)
      .i32_const(0)
      .i32x4_replace_lane(1)
      .i32_const(0)
      .i32x4_replace_lane(3);
  }

  pub fn push_number_as_v128(&mut self) {
    self
      .function
      .instructions()
      .i64_reinterpret_f64()
      .i64x2_splat()
      .i32_const(LoxType::Number as i32)
      .i32x4_replace_lane(0)
      .i32_const(0)
      .i32x4_replace_lane(1);
  }

  pub fn runtime_error(&mut self, error: ErrorMsg, token: &Token) -> Result<(), Error> {
    self.function
      .instructions()
      .i32_const(error.clone() as i32)
      .i32_const(token.loc.line_num.cast_signed()) // TODO: Pass in the relevant token for this
      .call(self.error_fn_index);
    Err((error, token.clone()))
  }
}

type Error = (ErrorMsg, Token);

#[derive(Clone, Debug, FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum ErrorMsg {
  OperandMustBeNumber,
  OperandsMustBeNumbers,
  OperandsMustBeNumsOrStrs,
  UndefinedVariable,
}

impl WasmCompiler {
  pub(super) const fn new() -> Self {
    Self { stack: ShadowStack::new() }
  }

  #[allow(clippy::too_many_lines)]
  pub(super) fn run(&mut self, ast: &Ast) -> Vec<u8> {
    let mut module = Module::new();

    let mut memories = MemorySection::new();
    let public_memory_index = memories.len();
    memories.memory(MemoryType {
      minimum: 1,
      maximum: None,
      memory64: false,
      shared: false,
      page_size_log2: None,
    });

    let data = DataSection::new();

    let mut types = TypeSection::new();

    let clock_type_index = types.len();
    types.ty().function([], [ValType::F64]);

    let error_type_index = types.len();
    types.ty().function([ValType::I32, ValType::I32], []);

    let print_int_type_index = types.len();
    types.ty().function([ValType::I32, ValType::I32], []);

    let print_number_type_index = types.len();
    types.ty().function([ValType::F64], []);

    let logical_type_index = types.len();
    types.ty().function([], [ValType::I64, ValType::I32]);

    let print_type_index = types.len();
    types.ty().function([ValType::I64, ValType::I32], []);

    let main_type_index = types.len();
    types.ty().function([], [ValType::I32, ValType::I32]);

    module.section(&types);

    let mut imports = ImportSection::new();
    let clock_fn_index = imports.len();
    imports.import("env", "clock", EntityType::Function(clock_type_index));
    let error_fn_index = imports.len();
    imports.import("env", "error", EntityType::Function(error_type_index));
    let print_int_fn_index = imports.len();
    imports.import("env", "print_int", EntityType::Function(print_int_type_index));
    let print_number_fn_index = imports.len();
    imports.import("env", "print_number", EntityType::Function(print_number_type_index));
    module.section(&imports);

    let mut functions = FunctionSection::new();
    let print_fn_index = imports.len() + functions.len();
    functions.function(print_type_index);
    let main_fn_index = imports.len() + functions.len();
    functions.function(main_type_index);
    module.section(&functions);

    module.section(&memories);

    let globals_map = find_scope_vars(&ast.statements);
    let mut globals = GlobalSection::new();
    for (_, value, _) in globals_map.values() {
      let gtype = GlobalType { val_type: ValType::V128, mutable: true, shared: false };
      globals.global(gtype, value);
    }
    module.section(&globals);

    let mut code = CodeSection::new();

    let mut print_fn = Function::new([]);
    print_fn.instructions()
      .block(BlockType::Empty)
        .block(BlockType::Empty)
          .block(BlockType::Empty)
            .block(BlockType::Empty)
              .block(BlockType::Empty)
                .local_get(1)
                .br_table(LoxType::iter().map(|t| t as u32), u32::try_from(LoxType::COUNT).unwrap())
              .end()
              .local_get(0) // 0: Nil
              .i32_wrap_i64()
              .i32_const(LoxType::Nil as i32)
              .call(print_int_fn_index)
              .br(4)
            .end()
            .local_get(0) // 1: Boolean
            .i32_wrap_i64()
            .i32_const(LoxType::Boolean as i32)
            .call(print_int_fn_index)
            .br(3)
          .end()
          .local_get(0) // 2: Number
          .f64_reinterpret_i64()
          .call(print_number_fn_index)
          .br(2)
        .end()
        .unreachable() // 3: Reference
        .br(1)
        .unreachable() // 4: No-match
      .end()
      .end();
    code.function(&print_fn);

    // TODO: This section duplicates the world of the globals-initializer above
    #[allow(clippy::collection_is_never_read)]
    let mut local_indices = HashMap::new();
    let mut locals_counter = 0;

    let constant_info: Vec<_> = ast
      .statements
      .iter()
      .filter_map(|stmt| {
        if let DeclareVar { new_var, initial: LiteralExpr { literal, .. }, .. } = stmt {
          let wasm_const = match literal {
            NilLit => WasmConst::I32(NIL),
            BooleanLit(false) => WasmConst::I32(Boolean::False as i32),
            BooleanLit(true) => WasmConst::I32(Boolean::True as i32),
            DoubleLit(x) => WasmConst::F64(Ieee64::new(x.to_bits())),
            StringLit(_str) => WasmConst::I32(Boolean::True as i32),
          };

          local_indices.insert(new_var.name.clone(), locals_counter);
          locals_counter += 1;

          Some(wasm_const)
        } else {
          None
        }
      })
      .collect();

    let fn_constant_defs: Vec<_> = constant_info.iter().map(WasmConst::val_type).map(|t| (1, t)).collect();
    let mut function = Function::new(fn_constant_defs);

    for (i, constant) in constant_info.iter().enumerate() {
      let mut instrs = function.instructions();
      constant.encode_push_into(&mut instrs);
      instrs.local_set(u32::try_from(i).unwrap());
    }

    let mut context = Context {
      compiler: self,
      depth: 0,
      clock_fn_index,
      error_fn_index,
      function,
      globals_map,
      logical_type_index,
      print_number_fn_index,
      print_fn_index,
      print_int_fn_index,
    };

    println!("===   DEBUG AST   ===");
    println!("{ast:#?}");
    println!("=== END DEBUG AST ===");

    {
      fn compile_statement(context: &mut Context, stmt: &Statement) -> Result<(), Vec<Error>> {
        match stmt {
          Block { contents, .. } => {
            context.depth += 1;

            // TODO: Push scope and establish new namespace
            let mut errors = Vec::new();
            for s in contents {
              if let Err(msgs) = compile_statement(context, s) {
                errors.extend(msgs);
              }
            }

            context.depth -= 1;

            if errors.is_empty() {
              Ok(())
            } else {
              Err(errors)
            }
          },

          Class { class_var: _, super_name_var_opt: _, methods: _, token: _ } => todo!("Not yet implemented"),

          DeclareVar { new_var: _nv, initial: _i, token: _t } => {
            // TODO: We can just skip this for now, since we just inline all of these up-front
            Ok(())
          },

          ExpressionStatement { expr, .. } => wrap_errs(compile_expr(context, expr)),

          ForStatement { initializer_opt, cond_opt, incrementer_opt, body, .. } => {
            context.function.instructions().block(BlockType::Empty); // Jump target: 1
            context.depth += 1;

            if let Some(initializer) = initializer_opt {
              compile_statement(context, initializer)?;
            }

            context.function.instructions().loop_(BlockType::Empty); // Jump target: 0

            if let Some(cond) = cond_opt {
              wrap_errs(compile_expr(context, cond))?;
              context.function.instructions().i32_eqz().br_if(1);
            }

            compile_statement(context, body)?;

            if let Some(incrementer) = incrementer_opt {
              wrap_errs(compile_expr(context, incrementer))?;
            }

            context.depth -= 1;
            context.function.instructions().br(0).end().end();

            Ok(())
          },

          FunctionStatement { func: _, token: _ } => todo!("Not yet implemented"),

          IfElse { antecedent, consequent, alternative_opt, .. } => {
            wrap_errs(compile_expr(context, antecedent))?;

            if context.compiler.stack.peek_nil(0) {
              context.function.instructions().drop();
              context.compiler.stack.pop();
              context.push_bool(false);
            } else if context.compiler.stack.peek_boolean(0) {
              context.function.instructions().i32_eqz().i32_eqz();
            } else if context.compiler.stack.peek_any(0) {
              todo!("`Any` go boom!");
            } else {
              context.function.instructions().drop();
              context.compiler.stack.pop();
              context.push_bool(true);
            }

            context.function.instructions().if_(BlockType::Empty);
            compile_statement(context, consequent)?;

            if let Some(alt) = alternative_opt {
              context.function.instructions().else_();
              compile_statement(context, alt)?;
            }

            context.function.instructions().end();

            Ok(())
          },

          PrintStatement { expr, .. } => {
            wrap_errs(compile_expr(context, expr))?;

            if context.compiler.stack.peek_number(0) {
              context.function.instructions().call(context.print_number_fn_index);
            } else if context.compiler.stack.peek_any(0) {
              context.function.instructions().call(context.print_fn_index);
            } else {
              context.push_type();
              context.compiler.stack.pop();
              context.function.instructions().call(context.print_int_fn_index);
            }

            context.compiler.stack.pop();

            Ok(())
          },

          ReturnStatement { expr_opt, .. } => wrap_errs(compile_return(context, expr_opt.as_ref())),

          WhileStatement { predicate, body, .. } => {
            context.function.instructions().block(BlockType::Empty).loop_(BlockType::Empty);

            wrap_errs(compile_expr(context, predicate))?;
            context.function.instructions().i32_eqz().br_if(1);

            compile_statement(context, body)?;

            context.function.instructions().br(0).end().end();

            Ok(())
          },
        }
      }

      fn compile_expr(context: &mut Context, expr: &Expr) -> Result<(), Error> {
        match expr {
          Assign { var, value, token } => {
            compile_expr(context, value)?;

            let name = var.name.clone();
            let depth = context.depth;

            // This is only for globals
            if let Some((index, value, _typ)) = extract_var(&mut context.globals_map, depth, &var.name) {
              if context.compiler.stack.peek_any(0) {
                panic!("Can this even happen?");
                //let _ = typ.take();
                //function
                //  .instructions()
                //  .i32x4_splat()
                //  .i32_const(0)
                //  .i32x4_replace_lane(1)
                //  .i64x2_replace_lane(1);
              } else if context.compiler.stack.peek_boolean(0) {
                context.push_boolean_as_v128();
                context.globals_map.insert((0, name), (index, value, LoxType::Boolean));
              } else if context.compiler.stack.peek_nil(0) {
                context.push_nil_as_v128();
                context.globals_map.insert((0, name), (index, value, LoxType::Nil));
              } else if context.compiler.stack.peek_number(0) {
                context.push_number_as_v128();
                context.globals_map.insert((0, name), (index, value, LoxType::Number));
              } else {
                todo!("Unhandled result type when setting global");
              }

              context.function.instructions().global_set(index);
              context.compiler.stack.pop();

              Ok(())
            } else {
              context.runtime_error(ErrorMsg::UndefinedVariable, token)
            }
          },

          Binary { left, right, token: Token { typ: BangEqual, loc } } => {
            let fake_eq = Binary {
              left: left.clone(),
              right: right.clone(),
              token: Token { typ: EqualEqual, loc: loc.clone() },
            };

            let res = compile_expr(context, &fake_eq);
            context.function.instructions().i32_eqz();

            res
          },

          Binary { left, right, token: Token { typ: EqualEqual, .. } } => {
            compile_expr(context, left)?;
            compile_expr(context, right)?;

            if context.compiler.stack.peek_boolean(0) && context.compiler.stack.peek_boolean(1) {
              context.compiler.stack.pop();
              context.function.instructions().i32_eq();
            } else if context.compiler.stack.peek_number(0) && context.compiler.stack.peek_number(1) {
              context.compiler.stack.pop();
              context.compiler.stack.pop();
              context
                .function
                .instructions()
                .f64_sub()
                .f64_abs()
                .f64_const(Ieee64::new((1e-9_f64).to_bits()))
                .f64_lt();
              context.compiler.stack.push_boolean();
            } else if context.compiler.stack.peek_reference(0) && context.compiler.stack.peek_reference(1) {
              todo!("Missing implementation");
              // let (a, b) = unsafe { (&*x, &*y) };
              // objs_are_equal(&a.object, &b.object)
            } else if context.compiler.stack.peek_nil(0) && context.compiler.stack.peek_nil(1) {
              context.compiler.stack.pop();
              context.compiler.stack.pop();
              context.push_bool(true);
            } else {
              context.compiler.stack.pop();
              context.compiler.stack.pop();
              context.push_bool(false);
            }

            Ok(())
          },

          Binary { left, right, token: token @ Token { typ: Greater, .. } } => {
            compile_expr(context, left)?;
            compile_expr(context, right)?;

            if context.compiler.stack.peek_number(0) && context.compiler.stack.peek_number(1) {
              context.function.instructions().f64_gt();
              context.compiler.stack.pop();
              context.compiler.stack.pop();
              context.compiler.stack.push_boolean();
              Ok(())
            } else {
              context.runtime_error(ErrorMsg::OperandsMustBeNumbers, token)
            }
          },

          Binary { left, right, token: Token { typ: GreaterEqual, loc } } => {
            let fake_greater = Binary {
              left: left.clone(),
              right: right.clone(),
              token: Token { typ: Greater, loc: loc.clone() },
            };

            let fake_equal = Binary {
              left: left.clone(),
              right: right.clone(),
              token: Token { typ: EqualEqual, loc: loc.clone() },
            };

            let fake_or = Logical {
              left: Box::new(fake_greater),
              right: Box::new(fake_equal),
              token: Token { typ: Or, loc: loc.clone() },
            };

            compile_expr(context, &fake_or)
          },

          Binary { left, right, token: token @ Token { typ: Less, .. } } => {
            compile_expr(context, left)?;
            compile_expr(context, right)?;

            if context.compiler.stack.peek_number(0) && context.compiler.stack.peek_number(1) {
              context.function.instructions().f64_lt();
              context.compiler.stack.pop();
              context.compiler.stack.pop();
              context.compiler.stack.push_boolean();
              Ok(())
            } else {
              context.runtime_error(ErrorMsg::OperandsMustBeNumbers, token)
            }
          },

          Binary { left, right, token: Token { typ: LessEqual, loc } } => {
            let fake_less = Binary {
              left: left.clone(),
              right: right.clone(),
              token: Token { typ: Less, loc: loc.clone() },
            };

            let fake_equal = Binary {
              left: left.clone(),
              right: right.clone(),
              token: Token { typ: EqualEqual, loc: loc.clone() },
            };

            let fake_or = Logical {
              left: Box::new(fake_less),
              right: Box::new(fake_equal),
              token: Token { typ: Or, loc: loc.clone() },
            };

            compile_expr(context, &fake_or)
          },

          Binary { left, right, token: token @ Token { typ: Minus, .. } } => {
            compile_expr(context, left)?;
            compile_expr(context, right)?;

            if context.compiler.stack.peek_number(0) && context.compiler.stack.peek_number(1) {
              context.function.instructions().f64_sub();
              context.compiler.stack.pop();
              Ok(())
            } else {
              context.runtime_error(ErrorMsg::OperandsMustBeNumbers, token)
            }
          },

          Binary { left, right, token: token @ Token { typ: Plus, .. } } => {
            compile_expr(context, left)?;
            compile_expr(context, right)?;

            if context.compiler.stack.peek_number(0) && context.compiler.stack.peek_number(1) {
              context.function.instructions().f64_add();
              context.compiler.stack.pop();
              Ok(())
            } else if context.compiler.stack.peek_string(0) && context.compiler.stack.peek_string(1) {
              todo!("String concatenation is not yet implemented");
              //   #[allow(irrefutable_let_patterns)]
              //   (Reference(GcPtr(x)), Reference(GcPtr(y)))
              //     if let GcObject { object: HeapString(str1), .. } = unsafe { &*x }
              //       && let GcObject { object: HeapString(str2), .. } = unsafe { &*y } =>
              //   {
              //     let _ = self.pop();
              //     let _ = self.pop();
              //     push_and_win!(Reference(GcPtr(self.compiler.heap.concatenate_strings(*str1, *str2).1)))
              //   },
            } else {
              context.runtime_error(ErrorMsg::OperandsMustBeNumsOrStrs, token)
            }
          },

          Binary { left, right, token: token @ Token { typ: Slash, .. } } => {
            compile_expr(context, left)?;
            compile_expr(context, right)?;

            if context.compiler.stack.peek_number(0) && context.compiler.stack.peek_number(1) {
              context.function.instructions().f64_div();
              context.compiler.stack.pop();
              Ok(())
            } else {
              context.runtime_error(ErrorMsg::OperandsMustBeNumbers, token)
            }
          },

          Binary { left, right, token: token @ Token { typ: Star, .. } } => {
            compile_expr(context, left)?;
            compile_expr(context, right)?;

            if context.compiler.stack.peek_number(0) && context.compiler.stack.peek_number(1) {
              context.function.instructions().f64_mul();
              context.compiler.stack.pop();
              Ok(())
            } else {
              context.runtime_error(ErrorMsg::OperandsMustBeNumbers, token)
            }
          },

          Binary { left: _, right: _, token } => todo!("This binary function is yet implemented: {token:?}"),

          Call { callee, arguments: _, token: _ } => {
            if let VarRef { var: Variable { name, .. }, .. } = &**callee {
              if resolve_var(&context.globals_map, context.depth, name).is_none() && name == "clock" {
                context.function.instructions().call(context.clock_fn_index);
                context.compiler.stack.push_number();
                Ok(())
              } else {
                todo!("It's currently only possible to call the special function `clock`");
              }
            } else {
              panic!("Impossible to use a non-variable as a function call target!");
            }
          },

          Get { object: _, var: _, token: _ } => todo!("Not yet implemented"),

          Grouping { expression, .. } => compile_expr(context, expression),

          LiteralExpr { literal, .. } => compile_literal(context, literal),

          Logical { left, right, token: Token { typ: And, .. } } => {
            compile_expr(context, left)?;

            if context.compiler.stack.peek_boolean(0) {
              context.function.instructions().if_(BlockType::FunctionType(context.logical_type_index));

              compile_expr(context, right)?;

              let consequent_is_bool = context.compiler.stack.peek_boolean(0);

              if context.compiler.stack.peek_any(0) {
                // Already encoded
              } else if consequent_is_bool {
                context.push_boolean_as_any();
              } else if context.compiler.stack.peek_nil(0) {
                context.push_nil_as_any();
              } else if context.compiler.stack.peek_number(0) {
                context.push_number_as_any();
              } else {
                todo!("Unhandled result type in `and`");
              }

              context
                .function
                .instructions()
                .else_()
                .i64_const(Boolean::False as i64)
                .i32_const(LoxType::Boolean as i32)
                .end();

              if consequent_is_bool {
                context.function.instructions().drop().i32_wrap_i64();
                context.compiler.stack.pop();
                context.compiler.stack.push_boolean();
              }
            } else if context.compiler.stack.peek_number(0) {
              context.function.instructions().drop();
              context.compiler.stack.pop();
              compile_expr(context, right)?;
            } else if context.compiler.stack.peek_nil(0) { // Auto-false
            } else if context.compiler.stack.peek_any(0) {
              todo!("Any go boom!");
            } else {
              todo!("Dunno what this is");
            }

            Ok(())
          },

          Logical { left, right, token: Token { typ: Or, .. } } => {
            compile_expr(context, left)?;

            if context.compiler.stack.peek_boolean(0) {
              context
                .function
                .instructions()
                .if_(BlockType::FunctionType(context.logical_type_index))
                .i64_const(Boolean::True as i64)
                .i32_const(LoxType::Boolean as i32)
                .else_();
              compile_expr(context, right)?;

              let antecedent_is_bool = context.compiler.stack.peek_boolean(0);

              if context.compiler.stack.peek_any(0) {
                // Already encoded
              } else if antecedent_is_bool {
                context.push_boolean_as_any();
              } else if context.compiler.stack.peek_nil(0) {
                context.push_nil_as_any();
              } else if context.compiler.stack.peek_number(0) {
                context.push_number_as_any();
              } else {
                todo!("Unhandled result type in `or`");
              }

              context.function.instructions().end();

              if antecedent_is_bool {
                context.function.instructions().drop().i32_wrap_i64();
                context.compiler.stack.pop();
                context.compiler.stack.push_boolean();
              }
            } else if context.compiler.stack.peek_nil(0) {
              context.function.instructions().drop();
              context.compiler.stack.pop();
              compile_expr(context, right)?;
            } else if context.compiler.stack.peek_number(0) { // Auto-true
            } else if context.compiler.stack.peek_any(0) {
              todo!("Any go boom!");
            } else {
              todo!("Dunno what this is");
            }

            Ok(())
          },

          Logical { token, .. } => {
            panic!("Illogical logical!  Got a: {token:?}");
          },

          Set { object: _, var: _, value: _, token: _ } => todo!("Not yet implemented"),
          Super { var: _, token: _ } => todo!("Not yet implemented"),
          This { token: _ } => todo!("Not yet implemented"),

          Unary { right, token: Token { typ: Bang, .. } } => {
            compile_expr(context, right)?;

            if context.compiler.stack.peek_nil(0) {
              context.function.instructions().drop();
              context.compiler.stack.pop();
              context.push_bool(true);
            } else if context.compiler.stack.peek_boolean(0) {
              context.function.instructions().i32_eqz();
            } else if context.compiler.stack.peek_any(0) {
              todo!("Any go boom!");
            } else {
              context.function.instructions().drop();
              context.compiler.stack.pop();
              context.push_bool(false);
            }

            Ok(())
          },

          Unary { right, token: token @ Token { typ: Minus, .. } } => {
            compile_expr(context, right)?;
            if context.compiler.stack.peek_number(0) {
              context.function.instructions().f64_neg();
              Ok(())
            } else {
              context.runtime_error(ErrorMsg::OperandMustBeNumber, token)
            }
          },

          Unary { token, .. } => {
            panic!("Disunitary unary!  Got a: {token:?}");
          },

          VarRef { var, token } => {
            // This is only for globals
            if let Some((index, _, typ)) = resolve_var(&context.globals_map, context.depth, &var.name) {
              context.function.instructions().global_get(*index);
              match typ {
                LoxType::Nil => {
                  context.function.instructions().drop();
                  context.push_nil();
                },
                LoxType::Boolean => {
                  context.function.instructions().i32x4_extract_lane(2);
                  context.compiler.stack.push_boolean();
                },
                LoxType::Number => {
                  context.function.instructions().i64x2_extract_lane(1).f64_reinterpret_i64();
                  context.compiler.stack.push_number();
                },
                LoxType::Reference => {
                  todo!("Retrieval for references is not yet implemented");
                },
              }
              Ok(())
            } else {
              context.runtime_error(ErrorMsg::UndefinedVariable, token)
            }
          },
        }
      }

      fn compile_return(context: &mut Context, expr_opt: Option<&Expr>) -> Result<(), Error> {
        if let Some(expr) = expr_opt {
          compile_expr(context, expr)?;
        } else {
          context.push_nil();
        }

        context.push_type();
        context.function.instructions().return_();
        context.compiler.stack.pop();
        context.compiler.stack.pop();

        Ok(())
      }

      // TODO: Remove `allow`
      #[allow(clippy::unnecessary_wraps)]
      fn compile_literal(context: &mut Context, literal: &Literal) -> Result<(), Error> {
        match literal {
          BooleanLit(value) => {
            context.push_bool(*value);
          },
          DoubleLit(value) => {
            context.push_number(*value);
          },
          StringLit(_value) => todo!("Not yet implemented"),
          NilLit => {
            context.push_nil();
          },
        }
        Ok(())
      }

      fn _compile_function(func: &AstFunction) -> Result<(), ErrorMsg> {
        let AstFunction { fn_decl: _, params: _, fn_body: _, token: _ } = func;
        todo!("Not yet implemented");
      }

      let mut errors = Vec::new();

      for stmt in &ast.statements {
        if let Err(msgs) = compile_statement(&mut context, stmt) {
          errors.extend(msgs);
        }
      }

      if let Err(msg) = compile_return(&mut context, None) {
        errors.push(msg);
      }

      assert!(errors.is_empty(), "Compilation to Wasm failed: {errors:?}");

      //  Named(Class) => {
      //    todo!("Not yet implemented: CLASS");
      //    // let (_, name_gc_ptr) = read_string!();
      //    // let x = self.compiler.heap.allocate_class(ClassObj::new(name_gc_ptr));
      //    // push_and_win!(Reference(GcPtr(x)))
      //  },

      //  Named(CloseUpvalue) => {
      //    todo!("Not yet implemented: CLOSEUPVALUE");
      //    // self.compiler.heap.close_upvalues(unsafe { self.stack_top.sub(1) });
      //    // self.pop();
      //    // Continue
      //  },

      //  Named(Closure) => {
      //    todo!("Not yet implemented: CLOSURE");
      //    // let constant = read_constant!();
      //    // if let Reference(GcPtr(fn_gc_ptr)) = constant
      //    //   && let GcObject { object, .. } = unsafe { &*fn_gc_ptr }
      //    //   && let HeapFunction(_) = object
      //    // {
      //    //   let (closure_ptr, closure_gc_ptr) = self.compiler.heap.allocate_closure(fn_gc_ptr);
      //    //   let result = push_and_win!(Reference(GcPtr(closure_gc_ptr)));

      //    //   let closure = unsafe { &*closure_ptr };

      //    //   let slots_ptr = frame!().slots_ptr;

      //    //   for i in 0..(closure.upvalue_count as usize) {
      //    //     let is_local = read_u8!();
      //    //     let index = read_u8!() as usize;

      //    //     if is_local == 1 {
      //    //       unsafe {
      //    //         let value_ptr = slots_ptr.add(index);
      //    //         *closure.upvalues_ptr_ptr.add(i) = self.capture_upvalue(value_ptr);
      //    //       }
      //    //     } else {
      //    //       let owning_closure = frame!().closure();
      //    //       unsafe { *closure.upvalues_ptr_ptr.add(i) = *owning_closure.upvalues_ptr_ptr.add(index) };
      //    //     }
      //    //   }

      //    //   result
      //    // } else {
      //    //   runtime_error!("Tried to read a function and got this: {constant:?}")
      //    // }
      //  },

      //  Named(FnCall) => {
      //    todo!("Not yet implemented: FNCALL");
      //    // let arg_count = read_u8!();
      //    // let value = self.peek(arg_count as usize);
      //    // self.call_value_for_error(&value, arg_count).unwrap_or(Continue)
      //  },

      //  Named(GetLocal) => {
      //    todo!("Not yet implemented: GETLOCAL");
      //    // let slot_num = read_u8!();
      //    // let slots_ptr = frame!().slots_ptr;
      //    // let value = unsafe { &*slots_ptr.add(slot_num as usize) }.clone();
      //    // push_and_win!(value)
      //  },

      //  Named(GetProperty) => {
      //    todo!("Not yet implemented: GETPROPERTY");
      //    // if let Reference(GcPtr(instance_gc_ptr)) = self.peek(0)
      //    //   && let HeapObjInstance(instance_obj_ptr) = unsafe { &*instance_gc_ptr }.object
      //    // {
      //    //   let instance_obj = unsafe { &*instance_obj_ptr };

      //    //   let (name, _) = read_string!();

      //    //   if let Some(property_value) = instance_obj.fields.get(name) {
      //    //     self.pop();
      //    //     push_and_win!(unsafe { &*property_value }.clone())
      //    //   } else if let Some(value_gc_ptr) = self.bind_method(instance_obj.class(), name) {
      //    //     push_and_win!(Reference(GcPtr(value_gc_ptr)))
      //    //   } else {
      //    //     runtime_error!("Undefined property '{name}'.")
      //    //   }
      //    // } else {
      //    //   runtime_error!("Only instances have properties.")
      //    // }
      //  },

      //  Named(GetSuper) => {
      //    todo!("Not yet implemented: GETSUPER");
      //    // let (name_str, _) = read_string!();

      //    // if let Reference(GcPtr(superclass_gc_ptr)) = self.pop()
      //    //   && let HeapClass(superclass_obj_ptr) = unsafe { &*superclass_gc_ptr }.object
      //    // {
      //    //   let superclass_obj = unsafe { &*superclass_obj_ptr };

      //    //   if let Some(value_gc_ptr) = self.bind_method(superclass_obj, name_str) {
      //    //     push_and_win!(Reference(GcPtr(value_gc_ptr)))
      //    //   } else {
      //    //     runtime_error!("Undefined property '{name_str}'.")
      //    //   }
      //    // } else {
      //    //   runtime_error!("Only instances can use `super`.")
      //    // }
      //  },

      //  Named(GetUpvalue) => {
      //    todo!("Not yet implemented: GETUPVALUE");
      //    // let slot = read_u8!() as usize;
      //    // let closure = frame_mut!().closure();
      //    // let HeapUpvalue(upvalue_ptr) = unsafe { &**closure.upvalues_ptr_ptr.add(slot) }.object else {
      //    //   panic!("Impossible for heap upvalue to be non-upvalue");
      //    // };
      //    // let value_ptr = unsafe { &*upvalue_ptr }.value_ptr;
      //    // let value = unsafe { &*value_ptr }.clone();
      //    // push_and_win!(value)
      //  },

      //  Named(Inherit) => {
      //    todo!("Not yet implemented: INHERIT");
      //    // if let Reference(GcPtr(super_gc_ptr)) = self.peek(1)
      //    //   && let HeapClass(super_class_obj_ptr) = unsafe { &*super_gc_ptr }.object
      //    //   && let superclass = unsafe { &mut *super_class_obj_ptr }
      //    // {
      //    //   if let Reference(GcPtr(sub_gc_ptr)) = self.peek(0)
      //    //     && let HeapClass(sub_class_obj_ptr) = unsafe { &*sub_gc_ptr }.object
      //    //     && let subclass = unsafe { &mut *sub_class_obj_ptr }
      //    //   {
      //    //     superclass.methods.copy_into(&mut subclass.methods);
      //    //     self.pop();
      //    //     Continue
      //    //   } else {
      //    //     runtime_error!("Subclass must be a class.")
      //    //   }
      //    // } else {
      //    //   runtime_error!("Superclass must be a class.")
      //    // }
      //  },

      //  Named(Invoke) => {
      //    todo!("Not yet implemented: INVOKE");
      //    // let (name_ptr, _) = read_string!();
      //    // let arg_count = read_u8!();
      //    // self.invoke(name_ptr, arg_count).unwrap_or(Continue)
      //  },

      //  Named(Loop) => {
      //    todo!("Not yet implemented: LOOP");
      //    //let offset = read_u16!() as usize;
      //    //let current = frame_mut!();
      //    //current.inst_ptr = unsafe { current.inst_ptr.sub(offset) };
      //    //Continue
      //  },

      //  Named(Method) => {
      //    todo!("Not yet implemented: METHODCODE");
      //    //let (_, method_name_gc_ptr) = read_string!();
      //    //self.define_method(method_name_gc_ptr);
      //    //Continue
      //  },

      //  Named(Pop) => {
      //    todo!("Not yet implemented: POP");
      //    //let _ = self.pop();
      //    //Continue
      //  },

      //  Named(SetLocal) => {
      //    todo!("Not yet implemented: SETLOCAL");
      //    //let slot_num = read_u8!();
      //    //let slots_ptr = frame!().slots_ptr;
      //    //let value = self.peek(0);
      //    //unsafe { *slots_ptr.add(slot_num as usize) = value };
      //    //Continue
      //  },

      //  Named(SetProperty) => {
      //    todo!("Not yet implemented: SETPROPERTY");
      //    //if let Reference(GcPtr(instance_gc_ptr)) = self.peek(1)
      //    //  && let HeapObjInstance(instance_obj_ptr) = unsafe { &*instance_gc_ptr }.object
      //    //{
      //    //  let instance_obj = unsafe { &mut *instance_obj_ptr };
      //    //  let (_, f_name_gc_ptr) = read_string!();
      //    //  instance_obj.fields.set(f_name_gc_ptr, self.peek(0));

      //    //  let value = self.pop();
      //    //  let _ = self.pop();
      //    //  push_and_win!(value)
      //    //} else {
      //    //  runtime_error!("Only instances have fields.")
      //    //}
      //  },

      //  Named(SetUpvalue) => {
      //    todo!("Not yet implemented: SETUPVALUE");
      //    //let slot = read_u8!() as usize;
      //    //let closure = frame_mut!().closure();
      //    //let HeapUpvalue(upvalue_ptr) = unsafe { &**closure.upvalues_ptr_ptr.add(slot) }.object else {
      //    //  panic!("Impossible for heap upvalue to be non-upvalue");
      //    //};
      //    //let value_ptr = unsafe { &*upvalue_ptr }.value_ptr;
      //    //let new_value = self.peek(0);
      //    //unsafe { *value_ptr = new_value };
      //    //Continue
      //  },

      //  Named(SuperInvoke) => {
      //    todo!("Not yet implemented: SUPERINVOKE");
      //    // let (name, _) = read_string!();
      //    // let arg_count = read_u8!();
      //    // let Reference(GcPtr(gc_ptr)) = self.pop() else {
      //    //   panic!("Super-invokee value must be a reference");
      //    // };
      //    // let HeapClass(class_obj_ptr) = unsafe { &*gc_ptr }.object else {
      //    //   panic!("Super-invokee value must be a class");
      //    // };
      //    // let class = unsafe { &*class_obj_ptr };
      //    // self.invoke_from_class(class, name, arg_count).unwrap_or(Continue)
      //  },
    }

    context.function.instructions().end();

    let mut exports = ExportSection::new();
    exports.export("main", ExportKind::Func, main_fn_index);
    exports.export("memory", ExportKind::Memory, public_memory_index);
    module.section(&exports);

    code.function(&context.function);
    module.section(&code);

    module.section(&data);

    module.finish()
  }
}

fn wrap_errs<T, U>(res: Result<T, U>) -> Result<T, Vec<U>> {
  match res {
    Err(msg) => Err(vec![msg]),
    Ok(x) => Ok(x),
  }
}
