use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub automatic_checks: bool,
    pub last_checked: i64,
    pub notified_version: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            automatic_checks: true,
            last_checked: 0,
            notified_version: String::new(),
        }
    }
}
