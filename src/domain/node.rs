use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Node {
    pub id: i32,
    pub host: String,
    pub port: i32,
}
