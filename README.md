rox
===============

## Based On...

https://craftinginterpreters.com/

This is a Rust-based implementation of `clox` (from the second half of the book, where you build a bytecode VM in C), and aims to become fully compliant with the `clox` suite of tests.

[![rox CI](https://github.com/TheBizzle/rox/actions/workflows/rox.yaml/badge.svg)](https://github.com/TheBizzle/rox/actions/workflows/rox.yaml)

## Testing

### Dart

Install Dart 2.x:

```sh
wget https://storage.googleapis.com/dart-archive/channels/stable/release/2.19.6/sdk/dartsdk-linux-x64-release.zip
unzip dartsdk-linux-x64-release.zip -d $HOME/Applications/dart-2.19.6
export PATH="$HOME/Applications/dart-2.19.6/dart-sdk/bin:$PATH"
```

Make sure to add that last line to your `.zshrc` (or equivalent), for permanent use.

### Building the tester

```sh
git submodule update --init
cd mothership
make get
cd ..
```

### Running the tests

```sh
./test.sh
```

## License

[Public domain / CC0-1.0](https://github.com/TheBizzle/rox/blob/main/LICENSE.txt)
