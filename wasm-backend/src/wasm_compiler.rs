use std::collections::{HashMap, VecDeque};

use strum::{EnumCount, EnumIter, FromRepr, IntoEnumIterator};

use wasm_encoder::{
  BlockType, CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function,
  FunctionSection, GlobalSection, GlobalType, Ieee64, ImportSection, InstructionSink, MemorySection,
  MemoryType, Module, TypeSection, ValType,
};

use rox_lib::core::byte::Byte::{self, Named, Raw};
use rox_lib::core::opcode::OpCode::{
  Add, Class, CloseUpvalue, Closure, Constant, DefineGlobal, Divide, Equal, False, FnCall, GetGlobal,
  GetLocal, GetProperty, GetSuper, GetUpvalue, Greater, Inherit, Invoke, Jump, JumpIfFalse, Less, Loop,
  Method, Multiply, Negate, Nil, Not, Pop, Print, Return, SetGlobal, SetLocal, SetProperty, SetUpvalue,
  Subtract, SuperInvoke, True,
};

use rox_lib::compiler::compilation::Compilation;
use rox_lib::compiler::compilation::CompiledValue::{
  self, CompiledBoolean, CompiledFunction, CompiledNil, CompiledNumber, CompiledString,
};

use super::shadow_stack::ShadowStack;

pub struct WasmCompiler {
  stack: ShadowStack,
}

#[derive(Debug)]
enum InitialValue {
  ConstReference(u8),
  Nil,
}

#[allow(dead_code)]
enum WasmConst {
  I32(i32),
  I64(i64),
  F64(Ieee64),
}

impl WasmConst {
  pub fn const_expr(&self) -> ConstExpr {
    match self {
      Self::I32(x) => ConstExpr::i32_const(*x),
      Self::I64(x) => ConstExpr::i64_const(*x),
      Self::F64(x) => ConstExpr::f64_const(*x),
    }
  }

  pub fn encode_push_into(&self, instrs: &mut InstructionSink) {
    match self {
      Self::I32(x) => instrs.i32_const(*x),
      Self::I64(x) => instrs.i64_const(*x),
      Self::F64(x) => instrs.f64_const(*x),
    };
  }

  pub const fn val_type(&self) -> ValType {
    match self {
      Self::I32(..) => ValType::I32,
      Self::I64(..) => ValType::I64,
      Self::F64(..) => ValType::F64,
    }
  }
}

#[derive(Clone, Debug, EnumCount, EnumIter, FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum Type {
  Nil,
  Boolean,
  Number,
  Reference,
  Raw,
}

impl Type {
  pub const fn from_compiled(value: &CompiledValue) -> Self {
    #[allow(clippy::match_same_arms)]
    match value {
      CompiledBoolean(..) => Self::Boolean,
      CompiledFunction { .. } => Self::Reference,
      CompiledNil => Self::Nil,
      CompiledNumber(..) => Self::Number,
      CompiledString(..) => Self::Reference,
    }
  }

  pub fn val_type(&self) -> ValType {
    match self {
      Self::Boolean | Self::Nil => ValType::I32,
      Self::Number => ValType::F64,
      _ => todo!("No such val_type... yet"),
    }
  }
}

const NIL: i32 = 0;

#[derive(FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum Boolean {
  False,
  True,
}

#[derive(FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum ErrorMsg {
  OperandMustBeNumber,
  OperandsMustBeNumbers,
  OperandsMustBeNumsOrStrs,
  UndefinedVariable,
}

enum ControlFlow {
  PartialIf,
  IfElse { target: usize },
  Or { target: usize },
  And { target: usize },
}
use ControlFlow::{And, IfElse, Or, PartialIf};

impl WasmCompiler {
  pub(super) const fn new() -> Self {
    Self { stack: ShadowStack::new() }
  }

  #[allow(clippy::too_many_lines)]
  pub(super) fn run(&mut self, compilation: &Compilation) -> Vec<u8> {
    let Compilation { strings: _strings, main: chunk } = compilation;
    let bytecode_pairs: &Vec<(u32, Byte)> =
      &chunk.line_data.iter().flat_map(|(i, xs)| xs.iter().map(|x| (*i, *x))).collect();

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

    let mut globals = GlobalSection::new();
    let global_ids: Vec<_> = chunk
      .line_data
      .values()
      .flatten()
      .collect::<Vec<_>>()
      .windows(3)
      .filter_map(|trio| {
        if let [Raw(init), Named(DefineGlobal), Raw(id)] = trio {
          Some((InitialValue::ConstReference(*init), id))
        } else if let [Named(Nil), Named(DefineGlobal), Raw(id)] = trio {
          Some((InitialValue::Nil, id))
        } else if let [x, Named(DefineGlobal), Raw(_id)] = trio {
          todo!("Unknown constant initializer: {x:?}");
        } else {
          None
        }
      })
      .collect();

    #[allow(clippy::match_same_arms)]
    let constant_info: Vec<_> = chunk
      .constants
      .iter()
      .map(|c| match c {
        CompiledBoolean(boolean) => {
          let value = if *boolean {
            Boolean::True
          } else {
            Boolean::False
          };
          WasmConst::I32(value as i32)
        },
        CompiledFunction { .. } => todo!("Function constants are not yet supported"),
        CompiledNil => WasmConst::I32(NIL),
        CompiledNumber(x) => WasmConst::F64(Ieee64::new(x.to_bits())),
        CompiledString(..) => WasmConst::I32(Boolean::True as i32),
      })
      .collect();

    let mut globals_map: HashMap<u8, (u32, ConstExpr, Option<Type>)> = HashMap::new();

    let mut globals_queue = VecDeque::from(global_ids);

    while let Some((init, id)) = globals_queue.pop_front() {
      let index = globals.len();
      let (gtype, gexpr, ltype) = match init {
        InitialValue::ConstReference(ref_id) if let Some((_, expr, lox_type)) = globals_map.get(&ref_id) => {
          let val_type = lox_type.as_ref().unwrap().val_type();
          let global_type = GlobalType { val_type, mutable: true, shared: false };
          (global_type, expr.clone(), lox_type.clone())
        },
        InitialValue::ConstReference(ref_id)
          if let Some(CompiledBoolean(_) | CompiledNil | CompiledNumber(..)) =
            chunk.constants.get(ref_id as usize) =>
        {
          let value = &constant_info[ref_id as usize];
          let val_type = value.val_type();
          let global_type = GlobalType { val_type, mutable: true, shared: false };
          let lox_type = Some(Type::from_compiled(&chunk.constants[ref_id as usize]));
          (global_type, value.const_expr(), lox_type)
        },
        InitialValue::ConstReference(_) => {
          globals_queue.push_back((init, id));
          break;
        },
        InitialValue::Nil => {
          let global_type = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
          (global_type, ConstExpr::i32_const(NIL), Some(Type::Nil))
        },
      };
      globals.global(gtype, &gexpr);
      globals_map.insert(*id, (index, gexpr, ltype));
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
                .br_table(Type::iter().map(|t| t as u32), u32::try_from(Type::COUNT).unwrap())
              .end()
              .local_get(0) // 0: Nil
              .i32_wrap_i64()
              .i32_const(Type::Nil as i32)
              .call(print_int_fn_index)
              .br(4)
            .end()
            .local_get(0) // 1: Boolean
            .i32_wrap_i64()
            .i32_const(Type::Boolean as i32)
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

    let fn_constant_defs: Vec<_> =
      constant_info.iter().map(WasmConst::val_type).map(|typ| (1, typ)).collect();
    let mut function = Function::new(fn_constant_defs);

    for (i, constant) in constant_info.iter().enumerate() {
      let mut instrs = function.instructions();
      constant.encode_push_into(&mut instrs);
      instrs.local_set(u32::try_from(i).unwrap());
    }

    macro_rules! push_type {
      () => {{
        let typ = if self.stack.peek_nil(0) {
          Type::Nil
        } else if self.stack.peek_boolean(0) {
          Type::Boolean
        } else if self.stack.peek_number(0) {
          Type::Number
        } else {
          panic!("Unsupported stack item type");
        };
        function.instructions().i32_const(typ as i32);
        self.stack.push_raw();
      }};
    }

    macro_rules! push_bool {
      ($boolean: ident) => {{
        function.instructions().i32_const(Boolean::$boolean as i32);
        self.stack.push_boolean();
      }};
    }

    macro_rules! push_nil {
      () => {{
        function.instructions().i32_const(NIL);
        self.stack.push_nil();
      }};
    }

    macro_rules! push_boolean_as_any {
      () => {{
        function.instructions().i64_extend_i32_u().i32_const(Type::Boolean as i32);
        self.stack.pop();
        self.stack.push_any();
      }};
    }

    macro_rules! push_nil_as_any {
      () => {{
        function.instructions().i64_extend_i32_u().i32_const(Type::Nil as i32);
        self.stack.pop();
        self.stack.push_any();
      }};
    }

    macro_rules! push_number_as_any {
      () => {{
        function.instructions().i64_reinterpret_f64().i32_const(Type::Number as i32);
        self.stack.pop();
        self.stack.push_any();
      }};
    }

    macro_rules! push_boolean_as_v128 {
      () => {{
        function
          .instructions()
          .i32x4_splat()
          .i32_const(0)
          .i32x4_replace_lane(1)
          .i32_const(0)
          .i32x4_replace_lane(3)
          .i32_const(Type::Boolean as i32)
          .i32x4_replace_lane(2);
      }};
    }

    macro_rules! push_nil_as_v128 {
      () => {{
        function
          .instructions()
          .i32x4_splat()
          .i32_const(0)
          .i32x4_replace_lane(1)
          .i32_const(0)
          .i32x4_replace_lane(3)
          .i32_const(Type::Nil as i32)
          .i32x4_replace_lane(2);
      }};
    }

    macro_rules! push_number_as_v128 {
      () => {{
        function
          .instructions()
          .i64_reinterpret_f64()
          .i64x2_splat()
          .i32_const(0)
          .i32x4_replace_lane(3)
          .i32_const(Type::Number as i32)
          .i32x4_replace_lane(2);
      }};
    }

    macro_rules! register_unknown {
      ($typ: expr) => {{
        match $typ {
          Some(Type::Boolean) => self.stack.push_boolean(),
          Some(Type::Nil) => self.stack.push_nil(),
          Some(Type::Number) => self.stack.push_number(),
          Some(Type::Raw) => todo!("Does this type even still exist?"),
          Some(Type::Reference) => todo!("References don't exist yet!"),
          None => self.stack.push_any(),
        }
      }};
    }

    println!("===   DEBUG BYTECODE   ===");
    for (i, code) in bytecode_pairs {
      print!("{i:>3} ");
      match code {
        Raw(num) => println!("{num}"),
        Named(x) => println!("{x}"),
      }
    }
    println!("=== END DEBUG BYTECODE ===");

    let mut cfs = Vec::new();
    let mut bc_index = 0;
    let mut this_is_fine = true;

    while this_is_fine && bc_index < bytecode_pairs.len() {
      match cfs.last() {
        Some(IfElse { target }) if &bc_index == target => {
          function.instructions().end();
          cfs.pop();
        },
        Some(Or { target }) if &bc_index == target => {
          let was_bool = self.stack.peek_boolean(0);

          if self.stack.peek_any(0) {
            // Already encoded
          } else if self.stack.peek_boolean(0) {
            push_boolean_as_any!();
          } else if self.stack.peek_nil(0) {
            push_nil_as_any!();
          } else if self.stack.peek_number(0) {
            push_number_as_any!();
          } else {
            todo!("Unhandled result type in `or`");
          }

          function.instructions().end();

          if was_bool {
            function.instructions().drop().i32_wrap_i64();
            self.stack.pop();
            self.stack.push_boolean();
          }

          cfs.pop();
        },
        Some(And { target }) if &bc_index == target => {
          let was_bool = self.stack.peek_boolean(0);

          if self.stack.peek_any(0) {
            // Already encoded
          } else if self.stack.peek_boolean(0) {
            push_boolean_as_any!();
          } else if self.stack.peek_nil(0) {
            push_nil_as_any!();
          } else if self.stack.peek_number(0) {
            push_number_as_any!();
          } else {
            todo!("Unhandled result type in `and`");
          }

          function
            .instructions()
            .else_()
            .i64_const(Boolean::False as i64)
            .i32_const(Type::Boolean as i32)
            .end();

          if was_bool {
            function.instructions().drop().i32_wrap_i64();
            self.stack.pop();
            self.stack.push_boolean();
          }

          cfs.pop();
        },
        _ => {},
      }

      let (line_num, code) = bytecode_pairs[bc_index];

      macro_rules! runtime_error {
        ($error_num: expr) => {{
          function
            .instructions()
            .i32_const($error_num)
            .i32_const(line_num.cast_signed())
            .call(error_fn_index);
          this_is_fine = false;
        }};
      }

      match code {
        Raw(num) => {
          function.instructions().i32_const(i32::from(num));
          self.stack.push_number();
        },
        Named(Add) => {
          if self.stack.peek_number(0) && self.stack.peek_number(1) {
            function.instructions().f64_add();
            self.stack.pop();
          } else if self.stack.peek_string(0) && self.stack.peek_string(1) {
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
            runtime_error!(ErrorMsg::OperandsMustBeNumsOrStrs as i32);
          }
        },

        Named(Class) => {
          todo!("Not yet implemented: CLASS");
          // let (_, name_gc_ptr) = read_string!();
          // let x = self.compiler.heap.allocate_class(ClassObj::new(name_gc_ptr));
          // push_and_win!(Reference(GcPtr(x)))
        },

        Named(CloseUpvalue) => {
          todo!("Not yet implemented: CLOSEUPVALUE");
          // self.compiler.heap.close_upvalues(unsafe { self.stack_top.sub(1) });
          // self.pop();
          // Continue
        },

        Named(Closure) => {
          todo!("Not yet implemented: CLOSURE");
          // let constant = read_constant!();
          // if let Reference(GcPtr(fn_gc_ptr)) = constant
          //   && let GcObject { object, .. } = unsafe { &*fn_gc_ptr }
          //   && let HeapFunction(_) = object
          // {
          //   let (closure_ptr, closure_gc_ptr) = self.compiler.heap.allocate_closure(fn_gc_ptr);
          //   let result = push_and_win!(Reference(GcPtr(closure_gc_ptr)));

          //   let closure = unsafe { &*closure_ptr };

          //   let slots_ptr = frame!().slots_ptr;

          //   for i in 0..(closure.upvalue_count as usize) {
          //     let is_local = read_u8!();
          //     let index = read_u8!() as usize;

          //     if is_local == 1 {
          //       unsafe {
          //         let value_ptr = slots_ptr.add(index);
          //         *closure.upvalues_ptr_ptr.add(i) = self.capture_upvalue(value_ptr);
          //       }
          //     } else {
          //       let owning_closure = frame!().closure();
          //       unsafe { *closure.upvalues_ptr_ptr.add(i) = *owning_closure.upvalues_ptr_ptr.add(index) };
          //     }
          //   }

          //   result
          // } else {
          //   runtime_error!("Tried to read a function and got this: {constant:?}")
          // }
        },

        Named(Constant) => {
          bc_index += 1;
          let (_, Raw(const_index)) = bytecode_pairs[bc_index] else {
            panic!("`Constant`'s operand must be a raw number");
          };

          // TODO: Have to actually check what kind of constant it is; functions will be handled
          // differently.
          function.instructions().local_get(u32::from(const_index));
          self.stack.push_number();
        },

        Named(DefineGlobal) => {
          function.instructions().drop();
          if self.stack.peek_any(0) {
            function.instructions().drop();
          }
          self.stack.pop();
          bc_index += 1;
        },

        Named(Divide) => {
          if self.stack.peek_number(0) && self.stack.peek_number(1) {
            function.instructions().f64_div();
            self.stack.pop();
          } else {
            runtime_error!(ErrorMsg::OperandsMustBeNumbers as i32);
          }
        },

        Named(Equal) => {
          if self.stack.peek_boolean(0) && self.stack.peek_boolean(1) {
            self.stack.pop();
            function.instructions().i32_eq();
          } else if self.stack.peek_number(0) && self.stack.peek_number(1) {
            self.stack.pop();
            self.stack.pop();
            function.instructions().f64_sub().f64_abs().f64_const(Ieee64::new((1e-9_f64).to_bits())).f64_lt();
            self.stack.push_boolean();
          } else if self.stack.peek_reference(0) && self.stack.peek_reference(1) {
            todo!("Missing implementation");
            // let (a, b) = unsafe { (&*x, &*y) };
            // objs_are_equal(&a.object, &b.object)
          } else if self.stack.peek_nil(0) && self.stack.peek_nil(1) {
            self.stack.pop();
            self.stack.pop();
            push_bool!(True);
          } else {
            self.stack.pop();
            self.stack.pop();
            push_bool!(False);
          }
        },

        Named(False) => {
          push_bool!(False);
        },

        Named(FnCall) => {
          todo!("Not yet implemented: FNCALL");
          // let arg_count = read_u8!();
          // let value = self.peek(arg_count as usize);
          // self.call_value_for_error(&value, arg_count).unwrap_or(Continue)
        },

        Named(GetGlobal) => {
          bc_index += 1;
          if let (_, Raw(id)) = bytecode_pairs[bc_index] {
            if let Some((index, _, typ)) = globals_map.get_mut(&id) {
              function.instructions().global_get(*index);
              if typ.is_none() {
                function
                  .instructions()
                  .i64x2_extract_lane(1)
                  .global_get(*index)
                  .i64x2_extract_lane(0)
                  .i32_wrap_i64();
              }
              register_unknown!(typ);
            } else {
              runtime_error!(ErrorMsg::UndefinedVariable as i32);
            }
          } else {
            panic!("Impossible global retrieval that isn't followed by ID");
          }
        },

        Named(GetLocal) => {
          todo!("Not yet implemented: GETLOCAL");
          // let slot_num = read_u8!();
          // let slots_ptr = frame!().slots_ptr;
          // let value = unsafe { &*slots_ptr.add(slot_num as usize) }.clone();
          // push_and_win!(value)
        },

        Named(GetProperty) => {
          todo!("Not yet implemented: GETPROPERTY");
          // if let Reference(GcPtr(instance_gc_ptr)) = self.peek(0)
          //   && let HeapObjInstance(instance_obj_ptr) = unsafe { &*instance_gc_ptr }.object
          // {
          //   let instance_obj = unsafe { &*instance_obj_ptr };

          //   let (name, _) = read_string!();

          //   if let Some(property_value) = instance_obj.fields.get(name) {
          //     self.pop();
          //     push_and_win!(unsafe { &*property_value }.clone())
          //   } else if let Some(value_gc_ptr) = self.bind_method(instance_obj.class(), name) {
          //     push_and_win!(Reference(GcPtr(value_gc_ptr)))
          //   } else {
          //     runtime_error!("Undefined property '{name}'.")
          //   }
          // } else {
          //   runtime_error!("Only instances have properties.")
          // }
        },

        Named(GetSuper) => {
          todo!("Not yet implemented: GETSUPER");
          // let (name_str, _) = read_string!();

          // if let Reference(GcPtr(superclass_gc_ptr)) = self.pop()
          //   && let HeapClass(superclass_obj_ptr) = unsafe { &*superclass_gc_ptr }.object
          // {
          //   let superclass_obj = unsafe { &*superclass_obj_ptr };

          //   if let Some(value_gc_ptr) = self.bind_method(superclass_obj, name_str) {
          //     push_and_win!(Reference(GcPtr(value_gc_ptr)))
          //   } else {
          //     runtime_error!("Undefined property '{name_str}'.")
          //   }
          // } else {
          //   runtime_error!("Only instances can use `super`.")
          // }
        },

        Named(GetUpvalue) => {
          todo!("Not yet implemented: GETUPVALUE");
          // let slot = read_u8!() as usize;
          // let closure = frame_mut!().closure();
          // let HeapUpvalue(upvalue_ptr) = unsafe { &**closure.upvalues_ptr_ptr.add(slot) }.object else {
          //   panic!("Impossible for heap upvalue to be non-upvalue");
          // };
          // let value_ptr = unsafe { &*upvalue_ptr }.value_ptr;
          // let value = unsafe { &*value_ptr }.clone();
          // push_and_win!(value)
        },

        Named(Greater) => {
          if self.stack.peek_number(0) && self.stack.peek_number(1) {
            function.instructions().f64_gt();
            self.stack.pop();
            self.stack.pop();
            self.stack.push_boolean();
          } else {
            runtime_error!(ErrorMsg::OperandsMustBeNumbers as i32);
          }
        },

        Named(Inherit) => {
          todo!("Not yet implemented: INHERIT");
          // if let Reference(GcPtr(super_gc_ptr)) = self.peek(1)
          //   && let HeapClass(super_class_obj_ptr) = unsafe { &*super_gc_ptr }.object
          //   && let superclass = unsafe { &mut *super_class_obj_ptr }
          // {
          //   if let Reference(GcPtr(sub_gc_ptr)) = self.peek(0)
          //     && let HeapClass(sub_class_obj_ptr) = unsafe { &*sub_gc_ptr }.object
          //     && let subclass = unsafe { &mut *sub_class_obj_ptr }
          //   {
          //     superclass.methods.copy_into(&mut subclass.methods);
          //     self.pop();
          //     Continue
          //   } else {
          //     runtime_error!("Subclass must be a class.")
          //   }
          // } else {
          //   runtime_error!("Superclass must be a class.")
          // }
        },

        Named(Invoke) => {
          todo!("Not yet implemented: INVOKE");
          // let (name_ptr, _) = read_string!();
          // let arg_count = read_u8!();
          // self.invoke(name_ptr, arg_count).unwrap_or(Continue)
        },

        Named(Jump) => {
          if matches!(cfs.last(), Some(PartialIf))
            && let Some((_, Raw(upper_bits))) = bytecode_pairs.get(bc_index + 1)
            && let Some((_, Raw(lower_bits))) = bytecode_pairs.get(bc_index + 2)
            && let Some((_, Named(Pop))) = bytecode_pairs.get(bc_index + 3)
          {
            let jump_distance = u16::from_be_bytes([*upper_bits, *lower_bits]);

            bc_index += 3;

            let len = cfs.len();
            cfs[len - 1] = IfElse { target: bc_index + (jump_distance as usize) };

            function.instructions().else_();
          } else {
            panic!("Unrecognized `else`-like control flow statement");
          }
        },

        Named(JumpIfFalse) => {
          if let Some((_, Raw(upper_bits))) = bytecode_pairs.get(bc_index + 1)
            && let Some((_, Raw(lower_bits))) = bytecode_pairs.get(bc_index + 2)
            && let Some((_, Named(Pop))) = bytecode_pairs.get(bc_index + 3)
          {
            bc_index += 3;
            let jump_distance = u16::from_be_bytes([*upper_bits, *lower_bits]);
            if let Some((_, Named(Jump))) = bytecode_pairs.get(bc_index - 3 + jump_distance as usize) {
              cfs.push(PartialIf);

              if self.stack.peek_nil(0) {
                function.instructions().drop();
                self.stack.pop();
                push_bool!(False);
              } else if self.stack.peek_boolean(0) {
                function.instructions().i32_eqz().i32_eqz();
              } else if self.stack.peek_any(0) {
                todo!("Any go boom!");
              } else {
                function.instructions().drop();
                self.stack.pop();
                push_bool!(True);
              }

              function.instructions().if_(BlockType::Empty);
            } else {
              // `and`
              if self.stack.peek_nil(0) {
                bc_index += usize::from(jump_distance - 1);
              } else if self.stack.peek_boolean(0) {
                cfs.push(And { target: bc_index + usize::from(jump_distance) });
                function.instructions().if_(BlockType::FunctionType(logical_type_index));
              } else if self.stack.peek_number(0) {
                function.instructions().drop();
                self.stack.pop();
              } else if self.stack.peek_any(0) {
                todo!("Any go boom!");
              } else {
                todo!("Dunno what this is");
              }
            }
          } else if let Some((_, Raw(_))) = bytecode_pairs.get(bc_index + 1)
            && let Some((_, Raw(_))) = bytecode_pairs.get(bc_index + 2)
            && let Some((_, Named(Jump))) = bytecode_pairs.get(bc_index + 3)
            && let Some((_, Raw(upper_bits))) = bytecode_pairs.get(bc_index + 4)
            && let Some((_, Raw(lower_bits))) = bytecode_pairs.get(bc_index + 5)
            && let Some((_, Named(Pop))) = bytecode_pairs.get(bc_index + 6)
          {
            bc_index += 6;
            if self.stack.peek_nil(0) {
              function.instructions().drop();
              self.stack.pop();
            } else if self.stack.peek_boolean(0) {
              let jump_distance = u16::from_be_bytes([*upper_bits, *lower_bits]);
              cfs.push(Or { target: bc_index + usize::from(jump_distance) });
              function
                .instructions()
                .if_(BlockType::FunctionType(logical_type_index))
                .i64_const(Boolean::True as i64)
                .i32_const(Type::Boolean as i32)
                .else_();
            } else if self.stack.peek_number(0) {
              let jump_distance = u16::from_be_bytes([*upper_bits, *lower_bits]);
              bc_index += usize::from(jump_distance - 1);
            } else if self.stack.peek_any(0) {
              todo!("Any go boom!");
            } else {
              todo!("Dunno what this is");
            }
          } else {
            panic!("Unrecognized `if`-like control flow statement");
          }
        },

        Named(Less) => {
          if self.stack.peek_number(0) && self.stack.peek_number(1) {
            function.instructions().f64_lt();
            self.stack.pop();
            self.stack.pop();
            self.stack.push_boolean();
          } else {
            runtime_error!(ErrorMsg::OperandsMustBeNumbers as i32);
          }
        },

        Named(Loop) => {
          todo!("Not yet implemented: LOOP");
          //let offset = read_u16!() as usize;
          //let current = frame_mut!();
          //current.inst_ptr = unsafe { current.inst_ptr.sub(offset) };
          //Continue
        },

        Named(Method) => {
          todo!("Not yet implemented: METHODCODE");
          //let (_, method_name_gc_ptr) = read_string!();
          //self.define_method(method_name_gc_ptr);
          //Continue
        },

        Named(Multiply) => {
          if self.stack.peek_number(0) && self.stack.peek_number(1) {
            function.instructions().f64_mul();
            self.stack.pop();
          } else {
            runtime_error!(ErrorMsg::OperandsMustBeNumbers as i32);
          }
        },

        Named(Negate) => {
          if self.stack.peek_number(0) {
            function.instructions().f64_neg();
          } else {
            runtime_error!(ErrorMsg::OperandMustBeNumber as i32);
          }
        },

        Named(Nil) => {
          push_nil!();
        },

        Named(Not) => {
          if self.stack.peek_nil(0) {
            function.instructions().drop();
            self.stack.pop();
            push_bool!(True);
          } else if self.stack.peek_boolean(0) {
            function.instructions().i32_eqz();
          } else if self.stack.peek_any(0) {
            todo!("Any go boom!");
          } else {
            function.instructions().drop();
            self.stack.pop();
            push_bool!(False);
          }
        },

        Named(Pop) => {
          todo!("Not yet implemented: POP");
          //let _ = self.pop();
          //Continue
        },

        Named(Print) => {
          if self.stack.peek_number(0) {
            function.instructions().call(print_number_fn_index);
          } else if self.stack.peek_any(0) {
            function.instructions().call(print_fn_index);
          } else {
            push_type!();
            self.stack.pop();
            function.instructions().call(print_int_fn_index);
          }
          self.stack.pop();
        },

        Named(Return) => {
          if self.stack.is_empty() {
            push_nil!();
          }
          push_type!();
          function.instructions().return_();
          self.stack.pop();
          self.stack.pop();
        },

        Named(SetGlobal) => {
          bc_index += 1;
          if let (_, Raw(id)) = bytecode_pairs[bc_index] {
            bc_index += 1;

            if let Some((index, _, typ)) = globals_map.get_mut(&id) {
              match typ {
                Some(Type::Reference) if self.stack.peek_nil(0) || self.stack.peek_reference(0) => {},
                Some(Type::Nil) if self.stack.peek_reference(0) => {
                  let _ = typ.replace(Type::Reference);
                },
                Some(Type::Nil) if self.stack.peek_nil(0) => {},
                Some(Type::Number) if self.stack.peek_number(0) => {},
                Some(Type::Boolean) if self.stack.peek_boolean(0) => {},
                Some(Type::Raw) => {
                  panic!("Impossible errant raw value");
                },
                None => {},
                _ => {
                  let _ = typ.take();
                },
              }

              if typ.is_none() {
                if self.stack.peek_any(0) {
                  function
                    .instructions()
                    .global_set(*index)
                    .i64x2_splat()
                    .i32_const(0)
                    .i32x4_replace_lane(3)
                    .local_get(*index)
                    .i64x2_replace_lane(0);
                } else if self.stack.peek_boolean(0) {
                  push_boolean_as_v128!();
                } else if self.stack.peek_nil(0) {
                  push_nil_as_v128!();
                } else if self.stack.peek_number(0) {
                  push_number_as_v128!();
                } else {
                  todo!("Unhandled result type when setting global");
                }
              }

              function.instructions().global_set(*index);
              self.stack.pop();
            } else {
              runtime_error!(ErrorMsg::UndefinedVariable as i32);
            }
          } else {
            panic!("Impossible global retrieval that isn't followed by ID");
          }
        },

        Named(SetLocal) => {
          todo!("Not yet implemented: SETLOCAL");
          //let slot_num = read_u8!();
          //let slots_ptr = frame!().slots_ptr;
          //let value = self.peek(0);
          //unsafe { *slots_ptr.add(slot_num as usize) = value };
          //Continue
        },

        Named(SetProperty) => {
          todo!("Not yet implemented: SETPROPERTY");
          //if let Reference(GcPtr(instance_gc_ptr)) = self.peek(1)
          //  && let HeapObjInstance(instance_obj_ptr) = unsafe { &*instance_gc_ptr }.object
          //{
          //  let instance_obj = unsafe { &mut *instance_obj_ptr };
          //  let (_, f_name_gc_ptr) = read_string!();
          //  instance_obj.fields.set(f_name_gc_ptr, self.peek(0));

          //  let value = self.pop();
          //  let _ = self.pop();
          //  push_and_win!(value)
          //} else {
          //  runtime_error!("Only instances have fields.")
          //}
        },

        Named(SetUpvalue) => {
          todo!("Not yet implemented: SETUPVALUE");
          //let slot = read_u8!() as usize;
          //let closure = frame_mut!().closure();
          //let HeapUpvalue(upvalue_ptr) = unsafe { &**closure.upvalues_ptr_ptr.add(slot) }.object else {
          //  panic!("Impossible for heap upvalue to be non-upvalue");
          //};
          //let value_ptr = unsafe { &*upvalue_ptr }.value_ptr;
          //let new_value = self.peek(0);
          //unsafe { *value_ptr = new_value };
          //Continue
        },

        Named(Subtract) => {
          if self.stack.peek_number(0) && self.stack.peek_number(1) {
            function.instructions().f64_sub();
            self.stack.pop();
          } else {
            runtime_error!(ErrorMsg::OperandsMustBeNumbers as i32);
          }
        },

        Named(SuperInvoke) => {
          todo!("Not yet implemented: SUPERINVOKE");
          // let (name, _) = read_string!();
          // let arg_count = read_u8!();
          // let Reference(GcPtr(gc_ptr)) = self.pop() else {
          //   panic!("Super-invokee value must be a reference");
          // };
          // let HeapClass(class_obj_ptr) = unsafe { &*gc_ptr }.object else {
          //   panic!("Super-invokee value must be a class");
          // };
          // let class = unsafe { &*class_obj_ptr };
          // self.invoke_from_class(class, name, arg_count).unwrap_or(Continue)
        },

        Named(True) => {
          push_bool!(True);
        },
      }

      bc_index += 1;
    }

    function.instructions().end();

    let mut exports = ExportSection::new();
    exports.export("main", ExportKind::Func, main_fn_index);
    exports.export("memory", ExportKind::Memory, public_memory_index);
    module.section(&exports);

    code.function(&function);
    module.section(&code);

    module.section(&data);

    module.finish()
  }
}
