use std::array;
use std::fmt::Arguments;
use std::process::exit;
use std::ptr::{self, null_mut};
use std::slice;
use std::sync::LazyLock;
use std::time::Instant;

use crate::compiler::{
  Compiler,
  FunctionKind::{self, Function, Method, Script},
};

use crate::disassembler::disassemble_instruction;

use crate::gc::FunctionObj::{MainScript, UserDefined};
use crate::gc::HeapObject::{
  HeapBoundMethod, HeapClass, HeapClosure, HeapFunction, HeapNativeFn, HeapObjInstance, HeapString,
  HeapUpvalue,
};
use crate::gc::{
  BoundMethodObj, ClassObj, ClosureObj, DEBUG_LOG_GC, DEBUG_STRESS_GC, Gc, GcObject, NativeFnObj,
  ObjInstanceObj, StringObj, UpvalueObj, objs_are_equal,
};

use crate::opcode::OpCode::{
  self, Add, Class, CloseUpvalue, Closure, Constant, DefineGlobal, Divide, Equal, False, FnCall, GetGlobal,
  GetLocal, GetProperty, GetSuper, GetUpvalue, Greater, Inherit, Invoke, Jump, JumpIfFalse, Less, Loop,
  Method as MethodCode, Multiply, Negate, Nil, Not, Pop, Print, Return, SetGlobal, SetLocal, SetProperty,
  SetUpvalue, Subtract, SuperInvoke, True,
};

use crate::memory::Freeable;

use crate::value::Value::{self, Boolean, Double, Nil as NilValue, Reference};

const FRAMES_MAX: usize = 64;
const IS_DEBUGGING: bool = false;
const STACK_MAX: usize = FRAMES_MAX * (u8::MAX as usize + 1);

static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

#[derive(Eq, PartialEq)]
pub enum Interpretation {
  CompilationError,
  RuntimeError,
  Success,
}
use Interpretation::{CompilationError, RuntimeError, Success};

#[derive(Debug)]
#[allow(clippy::struct_field_names)]
pub struct CallFrame {
  closure_gc_ptr: *mut GcObject,
  inst_ptr: *mut u8,
  slots_ptr: *mut Value,
}

impl CallFrame {
  fn closure(&self) -> &ClosureObj {
    if let HeapClosure(closure_ptr) = unsafe { &*self.closure_gc_ptr }.object {
      unsafe { &*closure_ptr }
    } else {
      panic!("VM's `closure_gc_ptr` must be a closure!");
    }
  }
}

impl Default for CallFrame {
  fn default() -> Self {
    Self { closure_gc_ptr: null_mut(), inst_ptr: null_mut(), slots_ptr: null_mut() }
  }
}

// Need to hold onto `_stack`, so Rust doesn't overwrite its memory --Jason B. (8/16/26)
pub struct VM {
  compiler: Compiler,
  current_frame_index: usize,
  frames: [CallFrame; FRAMES_MAX],
  instrs_since_last_gc: u16,
  _stack: Box<[Value; STACK_MAX]>,
  stack_addr: *mut Value,
  stack_top: *mut Value,
}

enum ProgressState {
  Continue,
  Done,
  Error,
}
use ProgressState::{Continue, Done, Error};

impl VM {
  #[allow(clippy::large_stack_frames)]
  #[must_use]
  pub fn init() -> Self {
    let frames: [CallFrame; FRAMES_MAX] = array::from_fn(|_| CallFrame::default());
    let mut stack: [Value; STACK_MAX] = array::from_fn(|_| Double(0.0));
    let stack_addr = stack.as_mut_ptr();
    let stack_top = stack.as_mut_ptr();

    let mut this = Self {
      compiler: Compiler::default(),
      current_frame_index: 0,
      frames,
      instrs_since_last_gc: 0,
      _stack: Box::new(stack),
      stack_addr,
      stack_top,
    };

    let _ = START_TIME;

    this.define_native_fn(
      "clock",
      NativeFnObj(Box::new(|_arg_count, _args_ptr| Double(START_TIME.elapsed().as_secs_f64()))),
    );

    this
  }

  pub fn collect_garbage(&mut self) {
    if DEBUG_LOG_GC {
      println!("-- gc begin");
    }

    self.mark_roots();
    self.compiler.gc.trace_references();
    self.compiler.gc.sweep();

    if DEBUG_LOG_GC {
      println!("-- gc end");
    }
  }

  #[must_use]
  /// # Panics
  ///
  /// When a lock cannot be acquired on the objects for GC'ing.
  pub fn free(&mut self) -> &Self {
    self.compiler.gc.free();
    self.compiler.gc = Gc::new();

    self
  }

  #[allow(clippy::option_if_let_else)]
  pub fn interpret(&mut self, source: String) -> Interpretation {
    self.compiler = Compiler::default();
    if let Some((_, function_gc_ptr)) = self.compiler.run(source) {
      self.push(Reference(function_gc_ptr));

      let (closure_ptr, closure_gc_ptr) = self.compiler.gc.allocate_closure(function_gc_ptr);
      let _ = self.pop();
      self.push(Reference(closure_gc_ptr));
      let _ = self.call_function_for_error(closure_ptr, closure_gc_ptr, 0, &Script);

      self.run()
    } else {
      CompilationError
    }
  }

  const fn peek(&mut self, distance: usize) -> Value {
    unsafe { ptr::read(self.stack_top.sub(distance + 1)) }
  }

  const fn pop(&mut self) -> Value {
    self.stack_top = unsafe { self.stack_top.sub(1) };
    unsafe { ptr::read(self.stack_top) }
  }

  fn push(&mut self, value: Value) {
    unsafe { *self.stack_top = value };
    self.stack_top = unsafe { self.stack_top.add(1) };
  }

  const fn reset_stack(&mut self) {
    self.stack_top = self.stack_addr;
    self.current_frame_index = 0;
  }

  #[allow(clippy::too_many_lines)]
  fn run(&mut self) -> Interpretation {
    macro_rules! runtime_error {
      ($($arg: tt)*) => {{
        self.runtime_error_impl(format_args!($($arg)*));
        Error
      }};
    }

    macro_rules! binary_op {
      ($value_type: tt, $op: tt) => {{
        match (self.peek(0), self.peek(1)) {
          (Double(b), Double(a)) => {
            let _      = self.pop();
            let _      = self.pop();
            let result = $value_type(a $op b);
            self.push(result);
            Continue
          },
          _ => {
            runtime_error!("Operands must be numbers.")
          }

        }
      }};
    }

    macro_rules! read_u8 {
      () => {{
        let current = &mut self.frames[self.current_frame_index];
        let byte = unsafe { *current.inst_ptr };
        current.inst_ptr = unsafe { current.inst_ptr.add(1) };
        byte
      }};
    }

    macro_rules! read_u16 {
      () => {{ u16::from_be_bytes([read_u8!(), read_u8!()]) }};
    }

    macro_rules! read_constant {
      () => {{
        let byte = read_u8!() as usize;
        let chunk = &mut self.frames[self.current_frame_index].closure().function().chunk();
        unsafe { ptr::read(chunk.constants.values.add(byte)) }
      }};
    }

    macro_rules! read_string {
      () => {{
        if let Reference(gc_ptr) = read_constant!()
          && let GcObject { object, .. } = unsafe { &*gc_ptr }
          && let HeapString(str_obj_ptr) = object
        {
          (str_obj_ptr.cast_const(), gc_ptr)
        } else {
          exit(1);
        }
      }};
    }

    macro_rules! push_and_win {
      ($x: expr) => {{
        let x = $x;
        self.push(x);
        Continue
      }};
    }

    loop {
      if IS_DEBUGGING {
        print!("          ");
        let size = unsafe { self.stack_top.offset_from(self.stack_addr) };
        let stack = unsafe { slice::from_raw_parts(self.stack_addr, size.cast_unsigned()) };
        for slot in stack {
          print!("[ {} ]", (*slot).stringify());
        }
        println!();

        let current = &self.frames[self.current_frame_index];
        let chunk = current.closure().function().chunk();
        let offset = unsafe { current.inst_ptr.offset_from(chunk.op_codes).cast_unsigned() };
        disassemble_instruction(chunk, offset);
      }

      let ordinal = read_u8!();

      let progress_state = match OpCode::from_repr(ordinal) {
        Some(Add) => {
          let a = self.peek(1);
          let b = self.peek(0);

          match (a, b) {
            (Double(x), Double(y)) => {
              let _ = self.pop();
              let _ = self.pop();
              push_and_win!(Double(x + y))
            },
            #[allow(irrefutable_let_patterns)]
            (Reference(x), Reference(y))
              if let GcObject { object: HeapString(str1), .. } = unsafe { &*x }
                && let GcObject { object: HeapString(str2), .. } = unsafe { &*y } =>
            {
              push_and_win!(Reference(self.compiler.gc.concatenate_strings(*str1, *str2).1))
            },
            _ => runtime_error!("Operands must be two numbers or two strings."),
          }
        },

        Some(Class) => {
          let (_, name_gc_ptr) = read_string!();
          let x = self.compiler.gc.allocate_class(ClassObj::new(name_gc_ptr));
          push_and_win!(Reference(x))
        },
        Some(CloseUpvalue) => {
          self.compiler.gc.close_upvalues(unsafe { self.stack_top.sub(1) });
          self.pop();
          Continue
        },
        Some(Closure) => {
          let constant = read_constant!();
          if let Reference(fn_gc_ptr) = constant
            && let GcObject { object, .. } = unsafe { &*fn_gc_ptr }
            && let HeapFunction(_) = object
          {
            let (closure_ptr, closure_gc_ptr) = self.compiler.gc.allocate_closure(fn_gc_ptr);
            let result = push_and_win!(Reference(closure_gc_ptr));

            let closure = unsafe { &*closure_ptr };

            let slots_ptr = self.frames[self.current_frame_index].slots_ptr;

            for i in 0..(closure.upvalue_count as usize) {
              let is_local = read_u8!();
              let index = read_u8!() as usize;

              if is_local == 1 {
                unsafe {
                  let value_ptr = slots_ptr.add(index);
                  *closure.upvalues_ptr_ptr.add(i) = self.capture_upvalue(value_ptr);
                }
              } else {
                let owning_closure = self.frames[self.current_frame_index].closure();
                unsafe { *closure.upvalues_ptr_ptr.add(i) = *owning_closure.upvalues_ptr_ptr.add(index) };
              }
            }

            result
          } else {
            runtime_error!("Tried to read a function and got this: {constant:?}")
          }
        },
        Some(Constant) => {
          push_and_win!(read_constant!())
        },

        Some(DefineGlobal) => {
          let value = self.peek(0);
          let (_, key_gc_ptr) = read_string!();
          self.compiler.gc.globals.set(key_gc_ptr, value);
          let _ = self.pop();
          Continue
        },
        Some(Divide) => binary_op!(Double, /),

        Some(Equal) => {
          let b = self.pop();
          let a = self.pop();
          push_and_win!(Boolean(values_are_equal(a, b)))
        },

        Some(False) => push_and_win!(Boolean(false)),
        Some(FnCall) => {
          let arg_count = read_u8!();
          let value = self.peek(arg_count as usize);
          self.call_value_for_error(&value, arg_count).unwrap_or(Continue)
        },

        Some(GetGlobal) => {
          let (name_ptr, _) = read_string!();
          if let Some(r) = self.compiler.gc.globals.get(name_ptr) {
            let value = unsafe { &*r }.clone();
            push_and_win!(value)
          } else {
            let name = unsafe { &*name_ptr };
            runtime_error!("Undefined variable '{}'.", name.to_text())
          }
        },
        Some(GetLocal) => {
          let slot_num = read_u8!();
          let slots_ptr = self.frames[self.current_frame_index].slots_ptr;
          let value = unsafe { &*slots_ptr.add(slot_num as usize) }.clone();
          push_and_win!(value)
        },
        Some(GetProperty) => {
          if let Reference(instance_gc_ptr) = self.peek(0)
            && let HeapObjInstance(instance_obj_ptr) = unsafe { &*instance_gc_ptr }.object
          {
            let instance_obj = unsafe { &*instance_obj_ptr };

            let (str_obj_ptr, _) = read_string!();

            if let Some(property_value) = instance_obj.fields.get(str_obj_ptr) {
              self.pop();
              push_and_win!(unsafe { &*property_value }.clone())
            } else if let Some(value_gc_ptr) = self.bind_method(instance_obj.class(), str_obj_ptr) {
              push_and_win!(Reference(value_gc_ptr))
            } else {
              let name_str = unsafe { &*str_obj_ptr };
              runtime_error!("Undefined property '{name_str}'.")
            }
          } else {
            runtime_error!("Only instances have properties.")
          }
        },
        Some(GetSuper) => {
          let (property_str_ptr, _) = read_string!();

          if let Reference(superclass_gc_ptr) = self.pop()
            && let HeapClass(superclass_obj_ptr) = unsafe { &*superclass_gc_ptr }.object
          {
            let superclass_obj = unsafe { &*superclass_obj_ptr };

            if let Some(value_gc_ptr) = self.bind_method(superclass_obj, property_str_ptr) {
              push_and_win!(Reference(value_gc_ptr))
            } else {
              let name_str = unsafe { &*property_str_ptr };
              runtime_error!("Undefined property '{name_str}'.")
            }
          } else {
            runtime_error!("Only instances can use `super`.")
          }
        },
        Some(GetUpvalue) => {
          let slot = read_u8!() as usize;
          let closure = &mut self.frames[self.current_frame_index].closure();
          let HeapUpvalue(upvalue_ptr) = unsafe { &**closure.upvalues_ptr_ptr.add(slot) }.object else {
            panic!("Impossible for heap upvalue to be non-upvalue");
          };
          let value_ptr = unsafe { &*upvalue_ptr }.value_ptr;
          let value = unsafe { &*value_ptr }.clone();
          push_and_win!(value)
        },
        Some(Greater) => binary_op!(Boolean, >),

        Some(Inherit) => {
          if let Reference(super_gc_ptr) = self.peek(1)
            && let HeapClass(super_class_obj_ptr) = unsafe { &*super_gc_ptr }.object
            && let superclass = unsafe { &mut *super_class_obj_ptr }
          {
            if let Reference(sub_gc_ptr) = self.peek(0)
              && let HeapClass(sub_class_obj_ptr) = unsafe { &*sub_gc_ptr }.object
              && let subclass = unsafe { &mut *sub_class_obj_ptr }
            {
              superclass.methods.copy_into(&mut subclass.methods);
              self.pop();
              Continue
            } else {
              runtime_error!("Subclass must be a class.")
            }
          } else {
            runtime_error!("Superclass must be a class.")
          }
        },
        Some(Invoke) => {
          let (name_ptr, _) = read_string!();
          let arg_count = read_u8!();
          self.invoke(name_ptr, arg_count).unwrap_or(Continue)
        },

        Some(Jump) => {
          let offset = read_u16!();
          let current = &mut self.frames[self.current_frame_index];
          unsafe {
            current.inst_ptr = current.inst_ptr.add(offset as usize);
          }
          Continue
        },
        Some(JumpIfFalse) => {
          let offset = read_u16!() as usize;
          let value = self.peek(0);
          let current = &mut self.frames[self.current_frame_index];
          if is_falsey(&value) {
            current.inst_ptr = unsafe { current.inst_ptr.add(offset) };
          }
          Continue
        },

        Some(Less) => binary_op!(Boolean, <),
        Some(Loop) => {
          let offset = read_u16!() as usize;
          let current = &mut self.frames[self.current_frame_index];
          current.inst_ptr = unsafe { current.inst_ptr.sub(offset) };
          Continue
        },

        Some(MethodCode) => {
          let (_, method_name_gc_ptr) = read_string!();
          self.define_method(method_name_gc_ptr);
          Continue
        },
        Some(Multiply) => binary_op!(Double, *),

        Some(Negate) => {
          if let Double(x) = self.peek(0) {
            let _ = self.pop();
            push_and_win!(Double(-x))
          } else {
            runtime_error!("Operand must be a number.")
          }
        },
        Some(Nil) => push_and_win!(Value::Nil),
        Some(Not) => {
          push_and_win!(Boolean(is_falsey(&self.pop())))
        },

        Some(Pop) => {
          let _ = self.pop();
          Continue
        },
        Some(Print) => {
          println!("{}", self.pop().stringify());
          Continue
        },

        Some(Return) => {
          let result = self.pop();
          self.compiler.gc.close_upvalues(self.frames[self.current_frame_index].slots_ptr);
          if self.current_frame_index == 0 {
            let _ = self.pop();
            Done
          } else {
            self.stack_top = self.frames[self.current_frame_index].slots_ptr;
            self.push(result);
            self.current_frame_index -= 1;
            Continue
          }
        },

        Some(SetGlobal) => {
          let (name_str_ptr, name_gc_ptr) = read_string!();
          let value = self.peek(0);
          let is_binding_new = self.compiler.gc.globals.set(name_gc_ptr, value);

          if is_binding_new {
            self.compiler.gc.globals.delete(name_str_ptr);
            runtime_error!("Undefined variable '{}'.", unsafe { &*name_str_ptr }.to_text())
          } else {
            Continue
          }
        },
        Some(SetLocal) => {
          let slot_num = read_u8!();
          let slots_ptr = self.frames[self.current_frame_index].slots_ptr;
          let value = self.peek(0);
          unsafe { *slots_ptr.add(slot_num as usize) = value };
          Continue
        },
        Some(SetProperty) => {
          if let Reference(instance_gc_ptr) = self.peek(1)
            && let HeapObjInstance(instance_obj_ptr) = unsafe { &*instance_gc_ptr }.object
          {
            let instance_obj = unsafe { &mut *instance_obj_ptr };
            let (_, f_name_gc_ptr) = read_string!();
            instance_obj.fields.set(f_name_gc_ptr, self.peek(0));

            let value = self.pop();
            let _ = self.pop();
            push_and_win!(value)
          } else {
            runtime_error!("Only instances have fields.")
          }
        },
        Some(SetUpvalue) => {
          let slot = read_u8!() as usize;
          let closure = &mut self.frames[self.current_frame_index].closure();
          let HeapUpvalue(upvalue_ptr) = unsafe { &**closure.upvalues_ptr_ptr.add(slot) }.object else {
            panic!("Impossible for heap upvalue to be non-upvalue");
          };
          let value_ptr = unsafe { &*upvalue_ptr }.value_ptr;
          let new_value = self.peek(0);
          unsafe { *value_ptr = new_value };
          Continue
        },
        Some(Subtract) => binary_op!(Double, -),
        Some(SuperInvoke) => {
          let (name_ptr, _) = read_string!();
          let arg_count = read_u8!();
          let Reference(gc_ptr) = self.pop() else {
            panic!("Super-invokee value must be a reference");
          };
          let HeapClass(class_obj_ptr) = unsafe { &*gc_ptr }.object else {
            panic!("Super-invokee value must be a class");
          };
          let class = unsafe { &*class_obj_ptr };
          self.invoke_from_class(class, name_ptr, arg_count).unwrap_or(Continue)
        },

        Some(True) => push_and_win!(Boolean(true)),

        None => {
          println!("Unknown instruction enum ordinal: {ordinal}");
          exit(1);
        },
      };

      if DEBUG_STRESS_GC {
        self.collect_garbage();
      } else if self.instrs_since_last_gc >= 999 {
        self.collect_garbage();
        self.instrs_since_last_gc = 0;
      } else {
        self.instrs_since_last_gc += 1;
      }

      match progress_state {
        Error => {
          return RuntimeError;
        },
        Done => {
          return Success;
        },
        Continue => {},
      }
    }
  }

  fn call_function_for_error(
    &mut self, closure_obj_ptr: *mut ClosureObj, closure_gc_ptr: *mut GcObject, arg_count: u8,
    function_kind: &FunctionKind,
  ) -> Option<ProgressState> {
    let closure = unsafe { &*closure_obj_ptr };
    let callee = closure.function();

    if u32::from(arg_count) == callee.arity() {
      if self.current_frame_index == (FRAMES_MAX - 1) {
        self.runtime_error_impl(format_args!("Stack overflow."));
        Some(Error)
      } else {
        if function_kind != &Script {
          self.current_frame_index += 1;
        }
        let current = &mut self.frames[self.current_frame_index];

        current.closure_gc_ptr = closure_gc_ptr;
        current.inst_ptr = callee.chunk().op_codes;
        current.slots_ptr = unsafe { self.stack_top.sub((arg_count + 1) as usize) };

        None
      }
    } else {
      self.runtime_error_impl(format_args!("Expected {} arguments but got {arg_count}.", callee.arity()));
      Some(Error)
    }
  }

  fn call_value_for_error(&mut self, callee: &Value, arg_count: u8) -> Option<ProgressState> {
    if let Reference(gc_ptr) = callee
      && let HeapBoundMethod(bound_method_ptr) = unsafe { &**gc_ptr }.object
      && !bound_method_ptr.is_null()
      && let bound_method_obj = unsafe { &*bound_method_ptr }
      && let HeapClosure(closure_obj_ptr) = unsafe { &*bound_method_obj.method_gc_ptr }.object
    {
      unsafe { *self.stack_top.sub((arg_count + 1) as usize) = bound_method_obj.receiver.clone() };
      self.call_function_for_error(closure_obj_ptr, bound_method_obj.method_gc_ptr, arg_count, &Function)
    } else if let Reference(gc_ptr) = callee
      && let HeapClass(class_obj_ptr) = unsafe { &**gc_ptr }.object
      && !class_obj_ptr.is_null()
    {
      let obj_instance_obj = ObjInstanceObj::new(*gc_ptr);
      let obj_instance_gc_ptr = self.compiler.gc.allocate_obj_instance(obj_instance_obj);
      unsafe { *self.stack_top.sub((arg_count + 1) as usize) = Reference(obj_instance_gc_ptr) };

      let HeapString(init_str_ptr) = unsafe { &*self.compiler.gc.init_str_gc_ptr }.object else {
        panic!("`init`'s name must be a string!");
      };

      let class_obj = unsafe { &*class_obj_ptr };
      if let Some(value_ptr) = class_obj.methods.get(init_str_ptr) {
        let Reference(closure_gc_ptr) = (unsafe { &*value_ptr }) else {
          panic!("`init` must be a reference!");
        };
        let HeapClosure(closure_obj_ptr) = unsafe { &**closure_gc_ptr }.object else {
          panic!("`init` must be a closure!");
        };
        self.call_function_for_error(closure_obj_ptr, *closure_gc_ptr, arg_count, &Method)
      } else if arg_count == 0 {
        None
      } else {
        self.runtime_error_impl(format_args!("Expected 0 arguments but got {arg_count}."));
        Some(Error)
      }
    } else if let Reference(gc_ptr) = callee
      && let HeapClosure(closure_obj_ptr) = unsafe { &**gc_ptr }.object
      && !closure_obj_ptr.is_null()
    {
      self.call_function_for_error(closure_obj_ptr, *gc_ptr, arg_count, &Function)
    } else if let Reference(gc_ptr) = callee
      && let HeapNativeFn(native_fn_ptr) = unsafe { &**gc_ptr }.object
      && !native_fn_ptr.is_null()
    {
      let native_fn = unsafe { &*native_fn_ptr };
      let result = native_fn.invoke(arg_count, unsafe { self.stack_top.sub(arg_count as usize) });
      unsafe { self.stack_top = self.stack_top.sub((arg_count + 1) as usize) };
      self.push(result);
      None
    } else {
      self.runtime_error_impl(format_args!("Can only call functions and classes."));
      Some(Error)
    }
  }

  fn capture_upvalue(&mut self, target_value_ptr: *mut Value) -> *mut GcObject {
    let mut prev_upvalue_opt = None;
    let mut upvalue_opt = self.compiler.gc.head_open_upvalue_gc_opt;

    while let Some(upvalue_gc_ptr) = upvalue_opt
      && let HeapUpvalue(upvalue_ptr) = unsafe { &*upvalue_gc_ptr }.object
      && let UpvalueObj { closed_value: NilValue, next_gc_opt, value_ptr } = unsafe { &*upvalue_ptr }
      && value_ptr > &target_value_ptr
    {
      prev_upvalue_opt = upvalue_opt.take();
      upvalue_opt = *next_gc_opt;
    }

    if let Some(upvalue_gc_ptr) = upvalue_opt
      && let HeapUpvalue(upvalue_ptr) = unsafe { &*upvalue_gc_ptr }.object
      && let upvalue = unsafe { &mut *upvalue_ptr }
      && upvalue.value_ptr == target_value_ptr
    {
      upvalue_gc_ptr
    } else {
      let upvalue_obj = UpvalueObj { closed_value: NilValue, next_gc_opt: None, value_ptr: target_value_ptr };
      let (_, allocated_gc_ptr) = self.compiler.gc.allocate_upvalue(upvalue_obj);

      if let Some(prev_upvalue_gc_ptr) = prev_upvalue_opt
        && let HeapUpvalue(upvalue_ptr) = unsafe { &*prev_upvalue_gc_ptr }.object
      {
        let prev_upvalue = unsafe { &mut *upvalue_ptr };
        prev_upvalue.next_gc_opt = Some(allocated_gc_ptr);
      } else {
        self.compiler.gc.head_open_upvalue_gc_opt = Some(allocated_gc_ptr);
      }

      allocated_gc_ptr
    }
  }

  pub fn define_native_fn(&mut self, name: &str, native_fn: NativeFnObj) {
    let (_, name_gc_ptr) = self.compiler.gc.copy_string(name, 0, name.len());
    let native_fn_gc_ptr = self.compiler.gc.allocate_native_fn(native_fn);

    let name_value = Reference(name_gc_ptr);
    let native_fn_value = Reference(native_fn_gc_ptr);
    self.push(name_value);
    self.push(native_fn_value.clone());

    self.compiler.gc.globals.set(name_gc_ptr, native_fn_value);

    self.pop();
    self.pop();
  }

  fn runtime_error_impl(&mut self, args: Arguments) {
    eprintln!("{args}");

    for i in (0..=self.current_frame_index).rev() {
      let frame = &mut self.frames[i];
      let function = frame.closure().function();
      let op_codes_ptr = function.chunk().op_codes;
      let instruction = unsafe { frame.inst_ptr.offset_from(op_codes_ptr) }.cast_unsigned() - 1;

      let suffix = match function {
        MainScript { .. } => "script".to_string(),
        UserDefined { name_gc_ptr, .. } => {
          let HeapString(name_ptr) = unsafe { &**name_gc_ptr }.object else {
            panic!("Not possible for name pointer to be non-string");
          };
          format!("{}()", unsafe { &*name_ptr }.to_text())
        },
      };

      let line_num = unsafe { &*function.chunk().line_nums.add(instruction) };
      eprintln!("[line {line_num}] in {suffix}");
    }

    self.reset_stack();
  }

  fn mark_roots(&mut self) {
    let size = unsafe { self.stack_top.offset_from(self.stack_addr).cast_unsigned() };
    let stack_slice = unsafe { slice::from_raw_parts(self.stack_addr, size) };

    for value in stack_slice {
      self.compiler.gc.mark_value(value);
    }

    for frame in &self.frames[0..=self.current_frame_index] {
      let gc_closure = unsafe { &mut *frame.closure_gc_ptr };
      self.compiler.gc.mark_object(gc_closure);
    }

    let mut upvalue_opt = self.compiler.gc.head_open_upvalue_gc_opt;
    while let Some(upvalue_gc_ptr) = upvalue_opt
      && let upvalue_gc = unsafe { &mut *upvalue_gc_ptr }
      && let HeapUpvalue(upvalue_ptr) = upvalue_gc.object
    {
      let upvalue = unsafe { &*upvalue_ptr };
      self.compiler.gc.mark_object(upvalue_gc);
      upvalue_opt = upvalue.next_gc_opt;
    }

    self.compiler.gc.mark_tables();
    self.compiler.mark_roots();
  }

  fn bind_method(&mut self, class: &ClassObj, key_ptr: *const StringObj) -> Option<*mut GcObject> {
    if let Some(method_ptr) = class.methods.get(key_ptr) {
      let Reference(ptr) = (unsafe { &*method_ptr }) else {
        panic!("Bound method must be a method!");
      };

      let bound_method_obj = BoundMethodObj { receiver: self.peek(0), method_gc_ptr: *ptr };
      let bound_method_ptr = self.compiler.gc.allocate_bound_method(bound_method_obj);
      self.pop();
      Some(bound_method_ptr)
    } else {
      None
    }
  }

  fn define_method(&mut self, name_ptr: *const GcObject) {
    let Reference(class_gc_ptr) = self.peek(1) else {
      panic!("Method's owner must be a reference!");
    };

    let HeapClass(class_obj_ptr) = unsafe { &*class_gc_ptr }.object else {
      panic!("Method's owner must be a class!");
    };

    let class_obj = unsafe { &mut *class_obj_ptr };
    class_obj.methods.set(name_ptr, self.peek(0));

    self.pop();
  }

  fn invoke(&mut self, name: *const StringObj, arg_count: u8) -> Option<ProgressState> {
    let receiver = self.peek(arg_count as usize);
    if let Reference(gc_ptr) = receiver
      && let HeapObjInstance(obj_instance_ptr) = unsafe { &*gc_ptr }.object
    {
      let obj_instance = unsafe { &*obj_instance_ptr };
      if let Some(value_ptr) = obj_instance.fields.get(name) {
        let value = unsafe { &*value_ptr };
        self.call_value_for_error(value, arg_count)
      } else {
        self.invoke_from_class(obj_instance.class(), name, arg_count)
      }
    } else {
      self.runtime_error_impl(format_args!("Only instances have methods."));
      Some(Error)
    }
  }

  fn invoke_from_class(
    &mut self, class: &ClassObj, name_ptr: *const StringObj, arg_count: u8,
  ) -> Option<ProgressState> {
    if let Some(value_ptr) = class.methods.get(name_ptr) {
      let Reference(closure_gc_ptr) = (unsafe { &*value_ptr }) else {
        panic!("Method must be a reference!");
      };
      let HeapClosure(closure_obj_ptr) = unsafe { &**closure_gc_ptr }.object else {
        panic!("Method must be a closure!");
      };
      self.call_function_for_error(closure_obj_ptr, *closure_gc_ptr, arg_count, &Method)
    } else {
      let name = unsafe { &*name_ptr };
      self.runtime_error_impl(format_args!("Undefined property '{name}'."));
      Some(Error)
    }
  }
}

const fn is_falsey(value: &Value) -> bool {
  matches!(value, NilValue | Boolean(false))
}

fn values_are_equal(a: Value, b: Value) -> bool {
  match (a, b) {
    (Boolean(x), Boolean(y)) => x == y,
    (Double(x), Double(y)) => (x - y).abs() < 1e-9,
    (Reference(x), Reference(y)) => {
      let (a, b) = unsafe { (&*x, &*y) };
      objs_are_equal(&a.object, &b.object)
    },
    (NilValue, NilValue) => true,
    _ => false,
  }
}
