use std::collections::{BTreeMap, HashMap};

use crate::runtime::byte::Byte;
use crate::runtime::chunk::Chunk;
use crate::runtime::gc_object::{GcObject, GcPtr};
use crate::runtime::heap_object::FunctionObj::{MainScript, UserDefined};
use crate::runtime::heap_object::HeapObject::{HeapFunction, HeapString};
use crate::runtime::value::Value::{self, Boolean, Double, Nil, Reference};
use crate::runtime::value_array::ValueArray;

pub struct StringRef(pub usize);

pub struct CompiledChunk {
  pub line_data: BTreeMap<u32, Vec<Byte>>,
  pub constants: Vec<CompiledValue>,
}

pub enum CompiledValue {
  CompiledBoolean(bool),
  CompiledNumber(f64),
  CompiledNil,
  CompiledFunction { arity: u32, upvalue_count: u16, name_ref: StringRef, chunk: CompiledChunk },
  CompiledString(StringRef),
}
use CompiledValue::{CompiledBoolean, CompiledFunction, CompiledNil, CompiledNumber, CompiledString};

pub struct Compilation {
  pub strings: Vec<String>,
  pub main: CompiledChunk,
}

type StringPointers = HashMap<*mut GcObject, String>;
type StringIndices = HashMap<*mut GcObject, usize>;

pub(super) fn crawl_root(gc_ptr: *mut GcObject) -> Compilation {
  let mut strings = Vec::new();
  let mut str_indices = HashMap::new();
  for (i, (ptr, str)) in find_str_info(gc_ptr).into_iter().enumerate() {
    strings.push(str);
    str_indices.insert(ptr, i);
  }

  if let HeapFunction(x) = unsafe { &*gc_ptr }.object
    && let MainScript { chunk } = unsafe { &*x }
  {
    let main = crawl_chunk(chunk, &str_indices);
    Compilation { strings, main }
  } else {
    panic!("Invalid successful compilation")
  }
}

fn crawl_chunk(chunk: &Chunk, str_indices: &StringIndices) -> CompiledChunk {
  let line_data = (0..(chunk.count))
    .map(|i| (unsafe { &*chunk.line_nums.add(i) }, unsafe { *chunk.op_codes.add(i) }))
    .fold(BTreeMap::<u32, Vec<_>>::new(), |mut treemap, (line_num, byte)| {
      treemap.entry(*line_num).or_default().push(byte);
      treemap
    });

  let constants = chunk.constants.iter().map(|x| crawl_value(x.clone(), str_indices)).collect();

  CompiledChunk { line_data, constants }
}

#[allow(clippy::needless_pass_by_value)]
fn crawl_value(value: Value, str_indices: &StringIndices) -> CompiledValue {
  match value {
    Boolean(x) => CompiledBoolean(x),
    Nil => CompiledNil,
    Double(num) => CompiledNumber(num),
    Reference(GcPtr(gc_ptr)) => match &unsafe { &*gc_ptr }.object {
      HeapFunction(ptr) => {
        let UserDefined { arity, chunk, name_gc_ptr, upvalue_count } = (unsafe { &**ptr }) else {
          panic!("Not possible to have top-level function here while serializing")
        };

        let HeapString(_) = unsafe { &**name_gc_ptr }.object else {
          panic!("Function name must be a string")
        };

        let name_ref = StringRef(*str_indices.get(name_gc_ptr).unwrap());
        let compiled_chunk = crawl_chunk(chunk, str_indices);
        CompiledFunction { arity: *arity, upvalue_count: *upvalue_count, name_ref, chunk: compiled_chunk }
      },
      HeapString(_) => CompiledString(StringRef(*str_indices.get(&gc_ptr).unwrap())),
      x => {
        panic!("Non-serializable: {x:?}")
      },
    },
  }
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
