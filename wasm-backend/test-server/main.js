const imports = {
  env: {
    print(type, value) {
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
          console.warn("Unknown print argument type ordinal: ", type);
        }
      }
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
