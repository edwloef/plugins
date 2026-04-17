use std::sync::atomic::{AtomicU32, Ordering};

mod whiteout;
mod dcc;
mod entry;

struct AtomicF32(AtomicU32);

impl AtomicF32 {
	fn new(value: f32) -> Self {
		Self(AtomicU32::new(value.to_bits()))
	}

	fn store(&self, value: f32) {
		self.0.store(value.to_bits(), Ordering::Relaxed)
	}

	fn load(&self) -> f32 {
		f32::from_bits(self.0.load(Ordering::Relaxed))
	}
}
