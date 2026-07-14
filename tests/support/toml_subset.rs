use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Str(String),
    Arr(Vec<String>),
}

impl Value {
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(value) => Some(value.as_str()),
            Self::Arr(_) => None,
        }
    }
}

pub(crate) type Table = BTreeMap<String, Value>;

/// Parse the shared quoted-string and quoted-string-array subset.
pub(crate) fn parse_value(rhs: &str) -> Result<Value, String> {
    if let Some(inner) = rhs.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let inner = inner.trim();
        if inner.is_empty() {
            return Ok(Value::Arr(Vec::new()));
        }
        let mut out = Vec::new();
        for part in inner.split(',') {
            let part = part.trim();
            let value = part
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or_else(|| format!("array element {part:?} must be a quoted string"))?;
            if value.contains('"') {
                return Err(format!("array element {part:?} has an embedded quote"));
            }
            out.push(value.to_string());
        }
        return Ok(Value::Arr(out));
    }
    let value = rhs
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| "value must be a quoted string or array".to_string())?;
    if value.contains('"') {
        return Err("string has an embedded quote".to_string());
    }
    Ok(Value::Str(value.to_string()))
}
