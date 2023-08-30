use std::collections::HashMap;
pub use tinyjson::JsonValue::{
    self, Array as JsonArray, Number as JsonNum, Object as JsonObj, String as JsonStr,
};

// helper functions
pub fn json_map<const N: usize>(vals: [(&str, JsonValue); N]) -> JsonValue {
    JsonObj(HashMap::from(vals.map(|(k, v)| (k.to_string(), v))))
}
pub fn json_str(val: &str) -> JsonValue {
    JsonStr(val.to_string())
}
