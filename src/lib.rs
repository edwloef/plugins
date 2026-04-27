use std::sync::atomic::{AtomicU32, Ordering};

mod dcc;
mod entry;
mod heater;
mod whiteout;

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

fn amp_to_db(amp: f32) -> f32 {
	20.0 * amp.log10()
}

fn db_to_amp(db: f32) -> f32 {
	10f32.powf(db / 20.0)
}
