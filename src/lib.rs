use std::sync::atomic::{AtomicU32, Ordering};

mod biquad;
mod cooler;
mod dcc;
mod entry;
mod heater;
mod vice;
mod whiteout;

struct Param {
	value: AtomicF32,
	r#mod: AtomicF32,
}

impl Param {
	fn new(value: f32) -> Self {
		Self {
			value: AtomicF32::new(value),
			r#mod: AtomicF32::new(0.0),
		}
	}

	fn store_value(&self, value: f32) {
		self.value.store(value);
	}

	fn store_mod(&self, r#mod: f32) {
		self.r#mod.store(r#mod);
	}

	fn load_value(&self) -> f32 {
		self.value.load()
	}

	fn load_combined(&self) -> f32 {
		self.value.load() + self.r#mod.load()
	}
}

struct AtomicF32(AtomicU32);

impl AtomicF32 {
	fn new(value: f32) -> Self {
		Self(AtomicU32::new(value.to_bits()))
	}

	fn store(&self, value: f32) {
		self.0.store(value.to_bits(), Ordering::Relaxed);
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

fn ms_to_coeff(secs: f32, sample_rate: f32) -> f32 {
	(-1000.0 / (secs * sample_rate)).exp()
}
