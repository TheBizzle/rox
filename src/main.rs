use std::env;

use tokio::io::{stdin, stdout};

#[tokio::main]
async fn main() {
  let _args: Vec<String> = env::args().collect();
  let _stdin = stdin();
  let _stdout = stdout();
}
