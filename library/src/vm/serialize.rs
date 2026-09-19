use std::fmt::Write;

use crate::core::byte::Byte;

use crate::compiler::compilation::CompiledValue::{
  self, CompiledBoolean, CompiledFunction, CompiledNil, CompiledNumber, CompiledString,
};
use crate::compiler::compilation::{Compilation, CompiledChunk, StringRef};

pub(super) fn serialize_root(compilation: &Compilation) -> String {
  let Compilation { strings, main } = compilation;

  let mut str_info = String::new();
  str_info.push_str("STRS\n");

  for str in strings {
    let _ = writeln!(str_info, "{str}\x1E");
  }
  str_info.push_str("Z_STRS");

  format!(
    "{str_info}

MAIN
{}
Z_MAIN
",
    serialize_chunk(main)
  )
}

fn serialize_chunk(chunk: &CompiledChunk) -> String {
  let line_data = chunk
    .line_data
    .iter()
    .map(|(line_num, bytes)| {
      let bytes_str = bytes.iter().map(serialize_byte).collect::<Vec<_>>().join(" ");
      format!("L{line_num} {bytes_str}")
    })
    .collect::<Vec<_>>()
    .join("\n");

  let const_data = chunk.constants.iter().map(serialize_value).collect::<Vec<_>>().join("\n");

  let newline_if_data = if const_data.is_empty() { "" } else { "\n" };

  format!(
    "CHUNK
{line_data}
CONSTS{newline_if_data}{const_data}"
  )
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_byte(byte: &Byte) -> String {
  match byte {
    Byte::Named(opcode) => opcode.to_string(),
    Byte::Raw(n) => n.to_string(),
  }
}

fn serialize_value(value: &CompiledValue) -> String {
  match value {
    CompiledBoolean(true) => "t".to_string(),
    CompiledBoolean(false) => "f".to_string(),
    CompiledNil => "n".to_string(),
    CompiledNumber(num) => format!("d{num}"),
    CompiledFunction { arity, upvalue_count, name_ref, chunk } => {
      format!(
        "FN
{arity} {upvalue_count} {}
{}
Z_FN",
        serialize_string_ref(name_ref),
        serialize_chunk(chunk)
      )
    },
    CompiledString(str_ref) => serialize_string_ref(str_ref),
  }
}

fn serialize_string_ref(str_ref: &StringRef) -> String {
  format!("s{}", str_ref.0)
}
