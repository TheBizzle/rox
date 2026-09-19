use crate::core::byte::Byte::{Named, Raw};
use crate::core::opcode::OpCode;

use crate::runtime::chunk::Chunk;
use crate::runtime::gc_object::{GcObject, GcPtr};
use crate::runtime::heap_object::FunctionObj::UserDefined;
use crate::runtime::value::Value::{self, Boolean, Double, Nil, Reference};

use crate::compiler::Compiler;

pub(super) fn deserialize(text: &str) -> Compiler {
  let mut compiler = Compiler::default();

  let mut text = text.strip_prefix("STRS\n").expect("Serialization must start with \"STRS\"");

  let mut strs = Vec::new();
  while !text.starts_with("Z_STRS") {
    let (result, new_text) = gobble_until(text, "\x1E\n");
    strs.push(result);
    text = new_text;
  }
  let text = text.strip_prefix("Z_STRS\n\nMAIN\n").expect("Must have \"MAIN\" section");

  let mut strings = Vec::new();
  for s in strs {
    strings.push(compiler.heap.copy_string_simple(&s, s.len()).1);
  }

  let (_, text) = deserialize_chunk(text, &mut compiler, &strings, true);

  let text = text.strip_prefix("Z_MAIN\n").expect("File must end with \"Z_MAIN\" and newline");
  assert!(text.is_empty(), "No text is allowed after `Z_MAIN`!");

  compiler
}

fn deserialize_chunk<'a>(
  text: &'a str, compiler: &mut Compiler, strings: &'a [*mut GcObject], is_root: bool,
) -> (Chunk, &'a str) {
  let mut text = text.strip_prefix("CHUNK\n").expect("Chunk must have \"CHUNK\" section");

  let mut chunk = Chunk::default();

  while !text.starts_with("CONSTS") {
    let (line, new_text) = gobble_until(text, "\n");
    text = new_text;

    let mut line = line;
    line = line.strip_prefix("L").expect("Opcode line must start with \"L\"").to_string();
    let (line_num_str, new_line) = gobble_until(&line, " ");
    line = new_line.to_string();
    let line_num = line_num_str.parse::<u32>().expect("Line number must be an integer");

    while !line.is_empty() {
      #[allow(clippy::option_if_let_else)]
      let (result, trimmed) = if let Some(delim_index) = line.find(' ') {
        (line[..delim_index].to_string(), &line[(delim_index + 1)..])
      } else {
        (line, "")
      };

      line = trimmed.to_string();

      let byte = result
        .parse::<OpCode>()
        .map(Named)
        .or_else(|_| result.parse::<u8>().map(Raw))
        .expect("Instruction must be opcode or int");

      if is_root {
        compiler.import_byte(byte, line_num);
      } else {
        chunk.write(byte, line_num);
      }
    }
  }

  let mut text = text.strip_prefix("CONSTS\n").expect("Op codes must be followed by \"CONSTS\" section");

  let (constants, new_text) = deserialize_constants(text, compiler, strings);
  text = new_text;

  if is_root {
    compiler.import_constants(constants);
  } else {
    for constant in constants {
      chunk.add_constant(constant);
    }
  }

  (chunk, text)
}

fn deserialize_constants<'a>(
  text: &'a str, compiler: &mut Compiler, strings: &'a [*mut GcObject],
) -> (Vec<Value>, &'a str) {
  let mut text = text;

  let mut constants = Vec::new();
  while !text.starts_with("Z_FN\n") && !text.starts_with("Z_MAIN\n") {
    if let Some(new_text) = text.strip_prefix("FN\n") {
      let (ptr, new_text) = deserialize_function(new_text, compiler, strings);
      constants.push(Reference(GcPtr(ptr)));
      text = new_text;
    } else if let Some(new_text) = text.strip_prefix("n\n") {
      constants.push(Nil);
      text = new_text;
    } else if let Some(new_text) = text.strip_prefix("t\n") {
      constants.push(Boolean(true));
      text = new_text;
    } else if let Some(new_text) = text.strip_prefix("f\n") {
      constants.push(Boolean(false));
      text = new_text;
    } else if let Some(new_text) = text.strip_prefix("s") {
      let (result, new_text) = gobble_until(new_text, "\n");
      let sid = result.parse::<u8>().expect("String must be IDed by integer");
      constants.push(Reference(GcPtr(strings[sid as usize])));
      text = new_text;
    } else if let Some(new_text) = text.strip_prefix("d") {
      let (result, new_text) = gobble_until(new_text, "\n");
      let num = result.parse::<f64>().expect("`d` must be followed by a number");
      constants.push(Double(num));
      text = new_text;
    } else {
      panic!("Impossible constant value: {text}");
    }
  }

  (constants, text)
}

fn deserialize_function<'a>(
  text: &'a str, compiler: &mut Compiler, strings: &'a [*mut GcObject],
) -> (*mut GcObject, &'a str) {
  let (line, text) = gobble_until(text, "\n");

  let (arity_str, line) = gobble_until(&line, " ");
  let arity = arity_str.parse::<u32>().expect("First segment of \"FN\" must be arity");

  let (upvalue_count_str, line) = gobble_until(line, " s");
  let upvalue_count =
    upvalue_count_str.parse::<u16>().expect("Second segment of \"FN\" must be upvalue count");

  let str_id = line.parse::<u8>().expect("Final segment of \"FN\" must be string ID");
  let name_gc_ptr = strings[str_id as usize];

  let (chunk, text) = deserialize_chunk(text, compiler, strings, false);
  let final_text = text.strip_prefix("Z_FN\n").unwrap();

  let fn_obj = UserDefined { arity, chunk, name_gc_ptr, upvalue_count };
  let gc_ptr = compiler.heap.allocate_function(fn_obj);

  (gc_ptr, final_text)
}

fn gobble_until<'a>(text: &'a str, target: &'a str) -> (String, &'a str) {
  let delim_index = text.find(target).expect("Did not find delimiter");
  let result = text[..delim_index].to_string();
  let trimmed = &text[(delim_index + target.len())..];
  (result, trimmed)
}
