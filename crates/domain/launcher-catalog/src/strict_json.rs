//! A JSON parser that refuses what ordinary parsers quietly accept.
//!
//! Most loaders keep the last of two equal object keys, so a signed document
//! can mean different things to different readers. This one rejects duplicate
//! keys, fractional and out-of-range numbers, excessive nesting, a byte-order
//! mark, and invalid UTF-8.

use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::limits::{MAX_JSON_DEPTH, MAX_SAFE_INTEGER};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StrictError {
	/// Not UTF-8, has a BOM, or is not syntactically JSON.
	Invalid(String),
	DuplicateKey,
}

// Marker texts let the visitor's string errors be told apart afterwards.
const DUPLICATE_MARKER: &str = "strict-json: duplicate key";

pub(crate) fn parse(bytes: &[u8]) -> Result<Value, StrictError> {
	let text =
		std::str::from_utf8(bytes).map_err(|error| StrictError::Invalid(error.to_string()))?;

	if text.starts_with('\u{feff}') {
		return Err(StrictError::Invalid("byte-order mark".into()));
	}

	let mut deserializer = serde_json::Deserializer::from_str(text);
	let value = Seed { depth: 0 }
		.deserialize(&mut deserializer)
		.map_err(classify)?;

	deserializer.end().map_err(classify)?;

	Ok(value)
}

fn classify(error: serde_json::Error) -> StrictError {
	let message = error.to_string();

	if message.contains(DUPLICATE_MARKER) {
		StrictError::DuplicateKey
	} else {
		StrictError::Invalid(message)
	}
}

struct Seed {
	depth: usize,
}

impl<'de> DeserializeSeed<'de> for Seed {
	type Value = Value;

	fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
		if self.depth > MAX_JSON_DEPTH {
			return Err(de::Error::custom("strict-json: nesting too deep"));
		}

		deserializer.deserialize_any(self)
	}
}

impl<'de> Visitor<'de> for Seed {
	type Value = Value;

	fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str("a JSON value without fractions or duplicate keys")
	}

	fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
		Ok(Value::Bool(value))
	}

	fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
		if value > MAX_SAFE_INTEGER {
			return Err(E::custom("strict-json: integer out of range"));
		}

		Ok(Value::Number(value.into()))
	}

	fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
		if value.unsigned_abs() > MAX_SAFE_INTEGER {
			return Err(E::custom("strict-json: integer out of range"));
		}

		Ok(Value::Number(Number::from(value)))
	}

	fn visit_f64<E: de::Error>(self, _: f64) -> Result<Value, E> {
		Err(E::custom("strict-json: fractional numbers are not allowed"))
	}

	fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
		Ok(Value::String(value.to_owned()))
	}

	fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
		Ok(Value::String(value))
	}

	fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
		Ok(Value::Null)
	}

	fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
		let mut items = Vec::new();

		while let Some(item) = sequence.next_element_seed(Seed {
			depth: self.depth + 1,
		})? {
			items.push(item);
		}

		Ok(Value::Array(items))
	}

	fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
		let mut map = Map::new();

		while let Some(key) = access.next_key::<String>()? {
			if map.contains_key(&key) {
				return Err(de::Error::custom(DUPLICATE_MARKER));
			}

			let value = access.next_value_seed(Seed {
				depth: self.depth + 1,
			})?;
			map.insert(key, value);
		}

		Ok(Value::Object(map))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_ordinary_documents() {
		let value = parse(br#"{"a":[1,2,{"b":"c"}],"d":null,"e":true}"#).unwrap();

		assert_eq!(value["a"][2]["b"], "c");
	}

	#[test]
	fn rejects_duplicate_keys_at_any_depth() {
		assert_eq!(parse(br#"{"a":1,"a":2}"#), Err(StrictError::DuplicateKey));
		assert_eq!(
			parse(br#"{"x":{"a":1,"b":2,"a":3}}"#),
			Err(StrictError::DuplicateKey)
		);
		assert_eq!(parse(br#"[{"a":1,"a":1}]"#), Err(StrictError::DuplicateKey));
	}

	#[test]
	fn rejects_fractions_and_exponents() {
		assert!(matches!(parse(b"1.5"), Err(StrictError::Invalid(_))));
		assert!(matches!(parse(b"1e3"), Err(StrictError::Invalid(_))));
		assert!(matches!(parse(b"1.0"), Err(StrictError::Invalid(_))));
	}

	#[test]
	fn enforces_the_interoperable_integer_range() {
		assert!(parse(b"9007199254740991").is_ok());
		assert!(parse(b"9007199254740992").is_err());
		assert!(parse(b"-9007199254740992").is_err());
	}

	#[test]
	fn rejects_excessive_nesting() {
		let deep = format!("{}{}", "[".repeat(100), "]".repeat(100));

		assert!(parse(deep.as_bytes()).is_err());
	}

	#[test]
	fn rejects_bad_encodings_and_trailing_data() {
		assert!(parse(&[0xff, 0xfe]).is_err());
		assert!(parse(b"\xef\xbb\xbf{}").is_err());
		assert!(parse(b"{} {}").is_err());
		assert!(parse(b"").is_err());
	}
}
