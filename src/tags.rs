use serde::ser::SerializeStruct;
use serde::Serializer;
use serde::{Deserialize, Serialize};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use base64::Engine;

use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct ValidationError(String);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tag parsing error: {}", self.0)
    }
}

impl Error for ValidationError {}

pub const TAGS_SCHEMA: &str = r#"{
    "type": "array",
    "items": {
      "type": "record",
      "name": "Tag",
      "fields": [
        { "name": "name", "type": "bytes" },
        { "name": "value", "type": "bytes" }
      ]
    }
  }"#;

#[derive(Debug, Deserialize)]
pub struct Tag {
    #[serde(with = "serde_bytes")]
    name: Vec<u8>,
    #[serde(with = "serde_bytes")]
    value: Vec<u8>,
}

impl Serialize for Tag {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Tag", 2)?;

        match (
            String::from_utf8(self.name.clone()),
            String::from_utf8(self.value.clone()),
        ) {
            (Ok(name), Ok(value)) => {
                state.serialize_field("name", &name)?;
                state.serialize_field("value", &value)?;
            }
            _ => {
                state.serialize_field("name", &BASE64_URL.encode(&self.name))?;
                state.serialize_field("value", &BASE64_URL.encode(&self.value))?;
            }
        }

        state.end()
    }
}

impl Tag {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.name.len() > 1024 {
            return Err(ValidationError("name exceeds 1024 bytes".into()));
        }
        if self.value.len() > 3072 {
            return Err(ValidationError("value exceeds 3072 bytes".into()));
        }
        if self.name.is_empty() || self.value.is_empty() {
            return Err(ValidationError("name and value must not be empty".into()));
        }
        Ok(())
    }

    pub fn try_to_utf8(&self) -> Result<(String, String), std::string::FromUtf8Error> {
        Ok((
            String::from_utf8(self.name.clone())?,
            String::from_utf8(self.value.clone())?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_success() {
        let tag = Tag {
            name: vec![b't', b'e', b's', b't'],
            value: vec![b'v', b'a', b'l', b'u', b'e'],
        };

        assert!(tag.validate().is_ok());
    }

    #[test]
    fn test_validate_name_too_long() {
        let tag = Tag {
            name: vec![b'a'; 1025],
            value: vec![b'v', b'a', b'l', b'u', b'e'],
        };

        let result = tag.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "tag parsing error: name exceeds 1024 bytes"
        );
    }

    #[test]
    fn test_validate_value_too_long() {
        let tag = Tag {
            name: vec![b't', b'e', b's', b't'],
            value: vec![b'b'; 3073],
        };

        let result = tag.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "tag parsing error: value exceeds 3072 bytes"
        );
    }

    #[test]
    fn test_validate_name_empty() {
        let tag = Tag {
            name: vec![],
            value: vec![b'v', b'a', b'l', b'u', b'e'],
        };

        let result = tag.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "tag parsing error: name and value must not be empty"
        );
    }

    #[test]
    fn test_validate_value_empty() {
        let tag = Tag {
            name: vec![b't', b'e', b's', b't'],
            value: vec![],
        };

        let result = tag.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "tag parsing error: name and value must not be empty"
        );
    }

    #[test]
    fn test_try_to_utf8_success() {
        let tag = Tag {
            name: vec![b't', b'e', b's', b't'],
            value: vec![b'v', b'a', b'l', b'u', b'e'],
        };

        let result = tag.try_to_utf8();
        assert!(result.is_ok());

        let (name, value) = result.unwrap();
        assert_eq!(name, "test");
        assert_eq!(value, "value");
    }

    #[test]
    fn test_try_to_utf8_invalid_utf8_name() {
        let tag = Tag {
            name: vec![0x80, 0x81, 0x82],
            value: vec![b'v', b'a', b'l', b'u', b'e'],
        };

        let result = tag.try_to_utf8();
        assert!(result.is_err());
    }

    #[test]
    fn test_try_to_utf8_invalid_utf8_value() {
        let tag = Tag {
            name: vec![b't', b'e', b's', b't'],
            value: vec![0x80, 0x81, 0x82],
        };

        let result = tag.try_to_utf8();
        assert!(result.is_err());
    }
}
