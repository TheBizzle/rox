#[derive(Clone, Debug)]
#[allow(unused)]
pub struct SourceLoc {
  pub start_index: u32,
  pub line_num: u32,
  pub column: u32,
  pub length: u32,
}

impl SourceLoc {
  pub fn extract(&self, source: &str) -> String {
    source.chars().skip(self.start_index as usize).take(self.length as usize).collect::<String>()
  }
}
