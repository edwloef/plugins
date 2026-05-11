use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub struct BiquadCoeffs {
	a1: f32,
	a2: f32,
	b0: f32,
	b1: f32,
	b2: f32,
}

impl BiquadCoeffs {
	#[must_use]
	pub fn highpass(sample_rate: f32, cutoff: f32, q: f32) -> Self {
		let omega = TAU * cutoff / sample_rate;
		let (sin_omega, cos_omega) = omega.sin_cos();
		let alpha = sin_omega / (2.0 * q);
		let b0 = (1.0 + cos_omega) / 2.0;
		let b1 = -(1.0 + cos_omega);
		let b2 = (1.0 + cos_omega) / 2.0;
		let a0 = 1.0 + alpha;
		let a1 = -2.0 * cos_omega;
		let a2 = 1.0 - alpha;
		Self {
			a1: a1 / a0,
			a2: a2 / a0,
			b0: b0 / a0,
			b1: b1 / a0,
			b2: b2 / a0,
		}
	}
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
	coeffs: BiquadCoeffs,
	x1: f32,
	x2: f32,
	y1: f32,
	y2: f32,
}

impl Biquad {
	#[must_use]
	pub fn new(coeffs: BiquadCoeffs) -> Self {
		Self {
			coeffs,
			x1: 0.0,
			x2: 0.0,
			y1: 0.0,
			y2: 0.0,
		}
	}

	#[must_use]
	pub fn tick(&mut self, x0: f32) -> f32 {
		let y0 = self.coeffs.b0 * x0 + self.coeffs.b1 * self.x1 + self.coeffs.b2 * self.x2
			- self.coeffs.a1 * self.y1
			- self.coeffs.a2 * self.y2;
		self.x2 = self.x1;
		self.x1 = x0;
		self.y2 = self.y1;
		self.y1 = y0;
		y0
	}

	pub fn reset(&mut self) {
		self.x1 = 0.0;
		self.x2 = 0.0;
		self.y1 = 0.0;
		self.y2 = 0.0;
	}
}
