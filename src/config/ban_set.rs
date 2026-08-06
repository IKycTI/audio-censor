use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
pub struct BanSet {
    pub version: u32,
    pub updated_at: String,
    pub words: HashSet<String>,
}

impl BanSet {
    pub fn new(file_path: &str) -> Result<Self, String> {
        let json_content = std::fs::read_to_string(file_path).map_err(|e| e.to_string())?;
        let my_s: Self = serde_json::from_str(&json_content).map_err(|e| e.to_string())?;
        Ok(my_s)
    }
}
