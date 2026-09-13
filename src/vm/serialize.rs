use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use crate::runtime::byte::Byte;
use crate::runtime::chunk::Chunk;
use crate::runtime::gc_object::{GcObject, GcPtr};
use crate::runtime::heap_object::FunctionObj::{MainScript, UserDefined};
use crate::runtime::heap_object::HeapObject::{HeapFunction, HeapString};
use crate::runtime::value::Value::{self, Boolean, Double, Nil, Reference};
use crate::runtime::value_array::ValueArray;

type StringPointers = HashMap<*mut GcObject, String>;
type StringIndices = HashMap<*mut GcObject, usize>;

pub(super) fn serialize_root(gc_ptr: *mut GcObject) -> String {
  let str_pointers = find_str_info(gc_ptr);

  let mut str_info = String::new();
  str_info.push_str("STRS\n");

  let mut str_indices = HashMap::new();
  for (i, (ptr, str)) in str_pointers.into_iter().enumerate() {
    let _ = writeln!(str_info, "{str}\x1E");
    str_indices.insert(ptr, i);
  }
  str_info.push_str("Z_STRS");

  if let HeapFunction(x) = unsafe { &*gc_ptr }.object
    && let MainScript { chunk } = unsafe { &*x }
  {
    format!(
      "{str_info}

MAIN
{}
Z_MAIN
",
      serialize_chunk(chunk, &str_indices)
    )
  } else {
    panic!("Invalid successful compilation")
  }
}

fn serialize_chunk(chunk: &Chunk, str_indices: &StringIndices) -> String {
  let chunk_data = (0..(chunk.count))
    .map(|i| (unsafe { &*chunk.line_nums.add(i) }, serialize_byte(unsafe { *chunk.op_codes.add(i) })))
    .fold(BTreeMap::<u32, Vec<_>>::new(), |mut treemap, (line_num, byte)| {
      treemap.entry(*line_num).or_default().push(byte);
      treemap
    })
    .into_iter()
    .map(|(line_num, bytes)| format!("L{line_num} {}", bytes.join(" ")))
    .collect::<Vec<_>>()
    .join("\n");

  let const_data =
    chunk.constants.iter().map(|x| serialize_value(x, str_indices)).collect::<Vec<_>>().join("\n");

  let newline_if_data = if const_data.is_empty() { "" } else { "\n" };

  format!(
    "CHUNK
{chunk_data}
CONSTS{newline_if_data}{const_data}"
  )
}

fn serialize_byte(byte: Byte) -> String {
  match byte {
    Byte::Named(opcode) => opcode.to_string(),
    Byte::Raw(n) => n.to_string(),
  }
}

fn serialize_value(value: &Value, str_indices: &StringIndices) -> String {
  match value {
    Boolean(true) => "t".to_string(),
    Boolean(false) => "f".to_string(),
    Nil => "n".to_string(),
    Double(num) => format!("d{num}"),
    Reference(GcPtr(gc_ptr)) => match &unsafe { &**gc_ptr }.object {
      HeapFunction(ptr) => {
        let UserDefined { arity, chunk, name_gc_ptr, upvalue_count } = (unsafe { &**ptr }) else {
          panic!("Not possible to have top-level function here while serializing")
        };

        let HeapString(_) = unsafe { &**name_gc_ptr }.object else {
          panic!("Function name must be a string")
        };

        format!(
          "FN
{arity} {upvalue_count} {}
{}
Z_FN",
          serialize_string_ptr(*name_gc_ptr, str_indices),
          serialize_chunk(chunk, str_indices)
        )
      },
      HeapString(_) => serialize_string_ptr(*gc_ptr, str_indices),
      x => {
        panic!("Non-serializable: {x:?}")
      },
    },
  }
}

fn serialize_string_ptr(gc_ptr: *mut GcObject, str_indices: &StringIndices) -> String {
  format!("s{}", str_indices.get(&gc_ptr).unwrap())
}

fn find_str_info(gc_ptr: *mut GcObject) -> StringPointers {
  if let HeapFunction(x) = unsafe { &*gc_ptr }.object
    && let MainScript { chunk, .. } = unsafe { &*x }
  {
    find_str_info_in_constants(&chunk.constants)
  } else {
    panic!("Invalid successful compilation")
  }
}

fn find_str_info_in_constants(constants: &ValueArray) -> StringPointers {
  constants.iter().fold(HashMap::new(), |mut acc, value| {
    let map = find_str_info_in_value(value);
    acc.extend(map);
    acc
  })
}

fn find_str_info_in_value(value: &Value) -> StringPointers {
  match value {
    Boolean(_) | Nil | Double(_) => HashMap::new(),
    Reference(GcPtr(gc_ptr)) => match &unsafe { &**gc_ptr }.object {
      HeapFunction(ptr) => {
        let UserDefined { chunk, name_gc_ptr, .. } = (unsafe { &**ptr }) else {
          panic!("Not possible to have top-level function here while serializing")
        };

        let HeapString(str_ptr) = unsafe { &**name_gc_ptr }.object else {
          panic!("Function name must be a string")
        };

        let mut fn_name_map = HashMap::from([(*name_gc_ptr, unsafe { &*str_ptr }.to_text())]);
        fn_name_map.extend(find_str_info_in_constants(&chunk.constants));
        fn_name_map
      },
      HeapString(ptr) => HashMap::from([(*gc_ptr, unsafe { &**ptr }.to_text())]),
      x => {
        panic!("Non-serializable: {x:?}")
      },
    },
  }
}
