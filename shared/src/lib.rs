use serde::{Deserialize, Serialize};
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct MouseMove {
    pub dx: f64,
    pub dy: f64,
}