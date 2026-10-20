use strum::FromRepr;

use wasm_encoder::{ConstExpr, Ieee64, InstructionSink, ValType};

pub const NIL: i32 = 0;

#[allow(dead_code)]
pub enum WasmConst {
  I32(i32),
  I64(i64), // TODO: Use it?
  F64(Ieee64),
}

impl WasmConst {
  pub const NIL: Self = Self::I32(NIL);

  pub fn as_v128(&self) -> ConstExpr {
    match self {
      Self::I32(x) => ConstExpr::v128_const(i128::from(*x) << 64),
      Self::I64(x) => ConstExpr::v128_const(i128::from(*x) << 64),
      Self::F64(x) => ConstExpr::v128_const(i128::from(x.bits()) << 64),
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

#[derive(FromRepr, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Boolean {
  False,
  True,
}
