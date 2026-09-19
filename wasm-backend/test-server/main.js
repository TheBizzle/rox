const imports = {
  env: {
    print_int(type, value) {
      switch(type) {
        case 0: {
          window.output.value += "nil";
          break;
        }
        case 1: {
          window.output.value += value === 0 ? "false" : "true";
          break;
        }
        default: {
          console.warn("Unknown int-print argument type ordinal: ", type, value);
        }
      }
      window.output.value += "\n";
    },

    print_number(type, value) {
      switch(type) {
        case 2: {
          window.output.value += value.toString();
          break;
        }
        default: {
          console.warn("Unknown numeric-print argument type ordinal: ", type, value);
        }
      }
      window.output.value += "\n";
    }
  }
};

const { instance } = await WebAssembly.instantiateStreaming(
  fetch("./program.wasm"),
  imports
);

const memory = instance.exports.memory;
const memoryView = new DataView(memory.buffer);

const result = instance.exports.main();
console.log(`Returned: ${result}`);
