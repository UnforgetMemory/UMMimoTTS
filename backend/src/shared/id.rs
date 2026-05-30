use uuid::Uuid;
use serde::{Serialize, Deserialize};
use std::fmt;
use std::hash::{Hash, Hasher};
use crate::shared::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Id(String);

impl Id {
    pub fn new() -> Self { Self(Uuid::now_v7().to_string()) }
    pub fn from_str(s: &str) -> Result<Self, AppError> {
        Uuid::parse_str(s).map_err(|_| AppError::InvalidInput(format!("Invalid ID: {}", s)))?;
        Ok(Self(s.to_string()))
    }
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn to_string(&self) -> String { self.0.clone() }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

impl Default for Id {
    fn default() -> Self { Self::new() }
}

impl PartialEq for Id {
    fn eq(&self, other: &Self) -> bool { self.0 == other.0 }
}

impl Eq for Id {}

impl Hash for Id {
    fn hash<H: Hasher>(&self, state: &mut H) { self.0.hash(state); }
}
