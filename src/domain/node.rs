use serde::Deserialize;


#[derive(Debug, Deserialize, Clone)]
pub struct Node {
    pub id: u32,
    pub host: String,
    pub port: u16,
}
