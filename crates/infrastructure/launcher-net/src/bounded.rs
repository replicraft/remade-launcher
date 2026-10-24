use launcher_model::{ErrorCode, LauncherError};

/// Collects response chunks up to a fixed size limit.
pub(crate) struct BoundedBuffer {
	limit: usize,
	data: Vec<u8>,
}

impl BoundedBuffer {
	/// `expected_len` is the advertised `Content-Length`, used only to
	/// pre-allocate. It is clamped, since a server can claim anything.
	pub(crate) fn new(limit: usize, expected_len: Option<u64>) -> Result<Self, LauncherError> {
		if let Some(expected) = expected_len
			&& expected > limit as u64
		{
			return Err(too_large(limit));
		}

		let capacity = expected_len.map_or(0, |len| len as usize).min(limit);

		Ok(Self {
			limit,
			data: Vec::with_capacity(capacity),
		})
	}

	pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<(), LauncherError> {
		if self.data.len().saturating_add(chunk.len()) > self.limit {
			return Err(too_large(self.limit));
		}

		self.data.extend_from_slice(chunk);

		Ok(())
	}

	pub(crate) fn into_vec(self) -> Vec<u8> {
		self.data
	}
}

fn too_large(limit: usize) -> LauncherError {
	LauncherError::new(
		ErrorCode::NetworkResponseTooLarge,
		format!("The server sent more than the allowed {limit} bytes."),
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn accepts_data_up_to_the_limit() {
		let mut buffer = BoundedBuffer::new(4, None).unwrap();

		buffer.push(b"ab").unwrap();
		buffer.push(b"cd").unwrap();

		assert_eq!(buffer.into_vec(), b"abcd");
	}

	#[test]
	fn rejects_data_past_the_limit() {
		let mut buffer = BoundedBuffer::new(3, None).unwrap();

		buffer.push(b"ab").unwrap();
		let error = buffer.push(b"cd").unwrap_err();

		assert_eq!(error.code(), ErrorCode::NetworkResponseTooLarge);
	}

	#[test]
	fn rejects_an_oversized_content_length_up_front() {
		assert!(BoundedBuffer::new(10, Some(11)).is_err());
	}

	#[test]
	fn a_lying_content_length_cannot_exhaust_memory() {
		// Claims exactly the limit, then sends too much.
		let mut buffer = BoundedBuffer::new(4, Some(4)).unwrap();

		assert!(buffer.push(b"abcde").is_err());
	}
}
