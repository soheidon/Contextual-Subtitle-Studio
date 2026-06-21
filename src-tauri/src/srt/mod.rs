pub mod parser;
pub mod writer;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubtitleEntry {
    pub index: u32,
    pub start: String,
    pub end: String,
    pub text: String,
}
