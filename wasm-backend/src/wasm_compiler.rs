use strum::FromRepr;

use wasm_encoder::{
  CodeSection, DataSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, Ieee64,
  ImportSection, MemorySection, MemoryType, Module, TypeSection, ValType,
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
  CompiledBoolean, CompiledFunction, CompiledNil, CompiledNumber, CompiledString,
};

use super::shadow_stack::ShadowStack;

pub struct WasmCompiler {
  stack: ShadowStack,
}

#[derive(FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum Type {
  Nil,
  Boolean,
  Number,
  _Reference,
  Raw,
}

const NIL: i32 = 0;

#[derive(FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum Boolean {
  False,
  True,
}

impl WasmCompiler {
  pub(super) const fn new() -> Self {
    Self { stack: ShadowStack::new() }
  }

  #[allow(clippy::too_many_lines)]
  pub(super) fn run(&mut self, compilation: &Compilation) -> Vec<u8> {
    let Compilation { strings: _strings, main: chunk } = compilation;
    let bytecode: &Vec<Byte> = &chunk.line_data.values().flat_map(Clone::clone).collect();

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

    let print_int_type_index = types.len();
    types.ty().function([ValType::I32, ValType::I32], []);

    let print_number_type_index = types.len();
    types.ty().function([ValType::I32, ValType::F64], []);

    let main_type_index = types.len();
    types.ty().function([], [ValType::I32, ValType::I32]);

    module.section(&types);

    let mut imports = ImportSection::new();
    let print_int_fn_index = imports.len();
    imports.import("env", "print_int", EntityType::Function(print_int_type_index));
    let print_number_fn_index = imports.len();
    imports.import("env", "print_number", EntityType::Function(print_number_type_index));
    module.section(&imports);

    let mut functions = FunctionSection::new();
    let main_fn_index = imports.len() + functions.len();
    functions.function(main_type_index);
    module.section(&functions);

    module.section(&memories);

    #[allow(clippy::match_same_arms)]
    let fn_constant_defs = chunk.constants.iter().map(|c| match c {
      CompiledBoolean(..) => (1, ValType::I32),
      CompiledFunction { .. } => todo!("Function constants are not yet supported"),
      CompiledNil => (1, ValType::I32),
      CompiledNumber(..) => (1, ValType::F64),
      CompiledString(..) => todo!("String constants are not yet supported"),
    });

    let mut code = CodeSection::new();
    let mut function = Function::new(fn_constant_defs);

    macro_rules! push_bool {
      ($boolean: ident) => {{
        function.instructions().i32_const(Type::Boolean as i32);
        function.instructions().i32_const(Boolean::$boolean as i32);
        self.stack.push_boolean();
      }};
    }

    macro_rules! push_nil {
      () => {{
        function.instructions().i32_const(Type::Nil as i32);
        function.instructions().i32_const(NIL);
        self.stack.push_nil();
      }};
    }

    for (i, constant) in chunk.constants.iter().enumerate() {
      let mut instrs = function.instructions();

      let instrs2 = match constant {
        CompiledBoolean(boolean) => {
          let value = if *boolean {
            Boolean::True
          } else {
            Boolean::False
          };
          instrs.i32_const(value as i32)
        },
        CompiledFunction { .. } => todo!("Function constants are not yet supported"),
        CompiledNil => instrs.i32_const(NIL),
        CompiledNumber(x) => instrs.f64_const(Ieee64::new(x.to_bits())),
        CompiledString(..) => todo!("String constants are not yet supported"),
      };

      instrs2.local_set(u32::try_from(i).unwrap());
    }

    println!("===   DEBUG BYTECODE   ===");
    for code in bytecode {
      match code {
        Raw(num) => println!("{num}"),
        Named(x) => println!("{x}"),
      }
    }
    println!("=== END DEBUG BYTECODE ===");

    let mut bc_index = 0;

    while bc_index < bytecode.len() {
      let code = bytecode[bc_index];

      match code {
        Raw(num) => {
          function.instructions().i32_const(Type::Raw as i32);
          function.instructions().i32_const(i32::from(num));
          self.stack.push_number();
        },
        Named(Add) => {
          todo!("Not yet implemented: ADD");
          // let a = self.peek(1);
          // let b = self.peek(0);

          // match (a, b) {
          //   (Double(x), Double(y)) => {
          //     let _ = self.pop();
          //     let _ = self.pop();
          //     push_and_win!(Double(x + y))
          //   },
          //   #[allow(irrefutable_let_patterns)]
          //   (Reference(GcPtr(x)), Reference(GcPtr(y)))
          //     if let GcObject { object: HeapString(str1), .. } = unsafe { &*x }
          //       && let GcObject { object: HeapString(str2), .. } = unsafe { &*y } =>
          //   {
          //     let _ = self.pop();
          //     let _ = self.pop();
          //     push_and_win!(Reference(GcPtr(self.compiler.heap.concatenate_strings(*str1, *str2).1)))
          //   },
          //   _ => runtime_error!("Operands must be two numbers or two strings."),
          // }
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
          let Raw(const_index) = bytecode[bc_index] else {
            panic!("`Constant`'s operand must be a raw number");
          };

          function.instructions().i32_const(Type::Number as i32);
          function.instructions().local_get(u32::from(const_index));
          self.stack.push_number();
        },

        Named(DefineGlobal) => {
          todo!("Not yet implemented: DEFINEGLOBAL");
          // let value = self.peek(0);
          // let (_, key_gc_ptr) = read_string!();
          // self.compiler.heap.globals.set(key_gc_ptr, value);
          // let _ = self.pop();
          // Continue
        },

        Named(Divide) => {
          todo!("Not yet implemented: DIVIDE");
          // binary_op!(Double, /)
        },

        Named(Equal) => {
          todo!("Not yet implemented: EQUAL");
          // let b = self.pop();
          // let a = self.pop();
          // push_and_win!(Boolean(values_are_equal(a, b)))
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
          todo!("Not yet implemented: GETGLOBAL");
          // let (name, _) = read_string!();
          // if let Some(r) = self.compiler.heap.globals.get(name) {
          //   let value = unsafe { &*r }.clone();
          //   push_and_win!(value)
          // } else {
          //   runtime_error!("Undefined variable '{}'.", name.to_text())
          // }
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
          todo!("Not yet implemented: GREATER");
          // binary_op!(Boolean, >)
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
          todo!("Not yet implemented: JUMP");
          // let offset = read_u16!();
          // let current = frame_mut!();
          // unsafe {
          //   current.inst_ptr = current.inst_ptr.add(offset as usize);
          // }
          // Continue
        },

        Named(JumpIfFalse) => {
          todo!("Not yet implemented: JUMPIFFALSE");
          //let offset = read_u16!() as usize;
          //let value = self.peek(0);
          //let current = frame_mut!();
          //if is_falsey(&value) {
          //  current.inst_ptr = unsafe { current.inst_ptr.add(offset) };
          //}
          //Continue
        },

        Named(Less) => {
          todo!("Not yet implemented: LESS");
          //binary_op!(Boolean, <)
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
          todo!("Not yet implemented: MULTIPLY");
          //binary_op!(Double, *)
        },

        Named(Negate) => {
          todo!("Not yet implemented: NEGATE");
          //if let Double(x) = self.peek(0) {
          //  let _ = self.pop();
          //  push_and_win!(Double(-x))
          //} else {
          //  runtime_error!("Operand must be a number.")
          //}
        },

        Named(Nil) => {
          push_nil!();
        },

        Named(Not) => {
          todo!("Not yet implemented: NOT");
          //push_and_win!(Boolean(is_falsey(&self.pop())))
        },

        Named(Pop) => {
          todo!("Not yet implemented: POP");
          //let _ = self.pop();
          //Continue
        },

        Named(Print) => {
          let fn_index = if self.stack.peek_number() {
            print_number_fn_index
          } else {
            print_int_fn_index
          };
          function.instructions().call(fn_index);
          self.stack.pop();
        },

        Named(Return) => {
          if self.stack.is_empty() {
            push_nil!();
          }
          self.stack.pop();
          function.instructions().return_();
        },

        Named(SetGlobal) => {
          todo!("Not yet implemented: SETGLOBAL");
          //let (name, name_gc_ptr) = read_string!();
          //let value = self.peek(0);
          //let is_binding_new = self.compiler.heap.globals.set(name_gc_ptr, value);

          //if is_binding_new {
          //  self.compiler.heap.globals.delete(name);
          //  runtime_error!("Undefined variable '{}'.", name.to_text())
          //} else {
          //  Continue
          //}
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
          todo!("Not yet implemented: SUBTRACT");
          //binary_op!(Double, -)
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
