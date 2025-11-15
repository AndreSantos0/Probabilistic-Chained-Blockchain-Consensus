use std::hash::Hash;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct Transaction {
    pub padding: Arc<str>,
}

impl Transaction {
    pub fn new(padding_size: usize) -> Self {
        let padding = Arc::<str>::from("X".repeat(padding_size));
        Transaction { padding }
    }
}

impl Serialize for Transaction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.padding.as_ref())
    }
}

impl<'de> Deserialize<'de> for Transaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let padding = String::deserialize(deserializer)?;
        Ok(Transaction { padding: Arc::<str>::from(padding) })
    }
}
