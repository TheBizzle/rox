const imports = {
  env: {
    error(error_num) {
      let msg = "UNKNOWN_ERROR";
      switch(error_num) {
        case 0: {
          msg = "Operand must be a number.";
          break;
        }
        default: {
          console.warn("Unknown error ordinal: ", error_num);
        }
      }
      window.output.value += `ERROR: ${msg}\n`;
    },

    print_int(value, type) {
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

    print_number(value) {
      window.output.value += `${value}\n`;
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
