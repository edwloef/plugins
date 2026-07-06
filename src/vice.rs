use crate::{Param, amp_to_db, db_to_amp, ms_to_coeff};
use clack_extensions::{
	audio_ports::{
		AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
		PluginAudioPortsImpl,
	},
	params::{
		HostParams, ParamDisplayWriter, ParamInfo, ParamInfoFlags, ParamInfoWriter,
		ParamRescanFlags, PluginAudioProcessorParams, PluginMainThreadParams, PluginParams,
	},
	state::{PluginState, PluginStateImpl},
};
use clack_plugin::{
	events::spaces::CoreEventSpace,
	plugin::features::{AUDIO_EFFECT, COMPRESSOR, STEREO},
	prelude::*,
	stream::{InputStream, OutputStream},
	utils::Cookie,
};
use std::{
	ffi::CStr,
	fmt::Write as _,
	io::{Read as _, Write as _},
};

pub struct Vice;

impl Vice {
	pub const ID: &'static str = "com.edwloef.vice";
}

impl Plugin for Vice {
	type AudioProcessor<'a> = AudioProcessor<'a>;
	type Shared<'a> = Shared;
	type MainThread<'a> = MainThread<'a>;

	fn declare_extensions(
		builder: &mut PluginExtensions<'_, Self>,
		_shared: Option<&Self::Shared<'_>>,
	) {
		builder
			.register::<PluginAudioPorts>()
			.register::<PluginParams>()
			.register::<PluginState>();
	}
}

impl DefaultPluginFactory for Vice {
	fn get_descriptor() -> PluginDescriptor {
		PluginDescriptor::new(Self::ID, "Vice")
			.with_version(env!("CARGO_PKG_VERSION"))
			.with_vendor("edwloef")
			.with_features([AUDIO_EFFECT, COMPRESSOR, STEREO])
	}

	fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
		Ok(Shared {
			ratio: Param::new(6.0),
			threshold: Param::new(-12.0),
			knee: Param::new(6.0),
			attack: Param::new(20.0),
			release: Param::new(100.0),
			postgain: Param::new(1.0),
		})
	}

	fn new_main_thread<'a>(
		host: HostMainThreadHandle<'a>,
		shared: &'a Self::Shared<'a>,
	) -> Result<Self::MainThread<'a>, PluginError> {
		Ok(MainThread { host, shared })
	}
}

pub struct AudioProcessor<'a> {
	shared: &'a Shared,
	sample_rate: f32,
	envelope: f32,
}

impl<'a> PluginAudioProcessor<'a, Shared, MainThread<'a>> for AudioProcessor<'a> {
	fn activate(
		_host: HostAudioProcessorHandle<'_>,
		_main_thread: &mut MainThread<'_>,
		shared: &'a Shared,
		audio_config: PluginAudioConfiguration,
	) -> Result<Self, PluginError> {
		Ok(Self {
			shared,
			sample_rate: audio_config.sample_rate as f32,
			envelope: 0.0,
		})
	}

	fn process(
		&mut self,
		_process: Process<'_>,
		mut audio: Audio<'_>,
		events: Events<'_>,
	) -> Result<ProcessStatus, PluginError> {
		let frames_count = audio.frames_count() as usize;
		let mut channels = audio
			.port_pair(0)
			.ok_or(PluginError::Message("No audio ports found"))?
			.channels()?
			.into_f32()
			.ok_or(PluginError::Message("No f32 channels provided"))?;

		for batch in events.input.batch() {
			self.shared.flush(batch.events());

			let ratio = self.shared.ratio.load_combined();
			let threshold = self.shared.threshold.load_combined();
			let knee = self.shared.knee.load_combined();
			let attack = ms_to_coeff(self.shared.attack.load_combined(), self.sample_rate);
			let release = ms_to_coeff(self.shared.release.load_combined(), self.sample_rate);
			let postgain = self.shared.postgain.load_combined();

			let low = threshold - 0.5 * knee;
			let high = threshold + 0.5 * knee;

			let mut gain = |max_abs: f32| -> f32 {
				let phase = if max_abs > self.envelope {
					attack
				} else {
					release
				};
				self.envelope = self.envelope * phase + max_abs * (1.0 - phase);
				let in_db = amp_to_db(self.envelope);
				let out_db = if in_db <= low {
					in_db
				} else if in_db >= high {
					threshold + (in_db - threshold) / ratio
				} else {
					in_db + (1.0 / ratio - 1.0) * (in_db - low).powi(2) / (2.0 * knee)
				};
				db_to_amp(out_db - in_db) * postgain
			};

			for i in batch.first_sample()..batch.next_batch_first_sample().unwrap_or(frames_count) {
				let mut max_abs = 0.0;

				for channel in channels.iter_mut() {
					match channel {
						ChannelPair::InputOnly(_) => {
							return Err(PluginError::Message("No output channel provided"));
						}
						ChannelPair::OutputOnly(_) => {
							return Err(PluginError::Message("No input channel provided"));
						}
						ChannelPair::InPlace(in_place) => max_abs = in_place[i].abs().max(max_abs),
						ChannelPair::InputOutput(input, _) => max_abs = input[i].abs().max(max_abs),
					}
				}

				let gain = gain(max_abs);

				for channel in channels.iter_mut() {
					match channel {
						ChannelPair::InputOnly(_) => {
							return Err(PluginError::Message("No output channel provided"));
						}
						ChannelPair::OutputOnly(_) => {
							return Err(PluginError::Message("No input channel provided"));
						}
						ChannelPair::InPlace(in_place) => in_place[i] *= gain,
						ChannelPair::InputOutput(input, output) => {
							output[i] = input[i] * gain;
						}
					}
				}
			}
		}

		Ok(if self.envelope >= f32::EPSILON {
			ProcessStatus::Continue
		} else {
			ProcessStatus::ContinueIfNotQuiet
		})
	}

	fn reset(&mut self) {
		self.envelope = 0.0;
	}
}

const PARAM_RATIO: ClapId = ClapId::new(0);
const PARAM_THRESHOLD: ClapId = ClapId::new(1);
const PARAM_KNEE: ClapId = ClapId::new(2);
const PARAM_ATTACK: ClapId = ClapId::new(3);
const PARAM_RELEASE: ClapId = ClapId::new(4);
const PARAM_POSTGAIN: ClapId = ClapId::new(5);

impl PluginAudioProcessorParams for AudioProcessor<'_> {
	fn flush(
		&mut self,
		input_parameter_changes: &InputEvents<'_>,
		_output_parameter_changes: &mut OutputEvents<'_>,
	) {
		self.shared.flush(input_parameter_changes);
	}
}

pub struct Shared {
	threshold: Param,
	knee: Param,
	ratio: Param,
	attack: Param,
	release: Param,
	postgain: Param,
}

impl Shared {
	fn flush<'a>(&self, input_parameter_changes: impl IntoIterator<Item = &'a UnknownEvent>) {
		for event in input_parameter_changes {
			match event.as_core_event() {
				Some(CoreEventSpace::ParamValue(event)) => match event.param_id() {
					Some(PARAM_RATIO) => self.ratio.store_value((event.value() as f32).powi(3)),
					Some(PARAM_THRESHOLD) => self.threshold.store_value(event.value() as f32),
					Some(PARAM_KNEE) => self.knee.store_value(event.value() as f32),
					Some(PARAM_ATTACK) => self.attack.store_value((event.value() as f32).powi(3)),
					Some(PARAM_RELEASE) => self.release.store_value((event.value() as f32).powi(3)),
					Some(PARAM_POSTGAIN) => {
						self.postgain.store_value(db_to_amp(event.value() as f32));
					}
					_ => {}
				},
				Some(CoreEventSpace::ParamMod(event)) => match event.param_id() {
					Some(PARAM_RATIO) => self.ratio.store_mod((event.amount() as f32).powi(3)),
					Some(PARAM_THRESHOLD) => self.threshold.store_mod(event.amount() as f32),
					Some(PARAM_KNEE) => self.knee.store_mod(event.amount() as f32),
					Some(PARAM_ATTACK) => self.attack.store_mod((event.amount() as f32).powi(3)),
					Some(PARAM_RELEASE) => self.release.store_mod((event.amount() as f32).powi(3)),
					Some(PARAM_POSTGAIN) => {
						self.postgain.store_mod(db_to_amp(event.amount() as f32));
					}
					_ => {}
				},
				_ => {}
			}
		}
	}
}

impl PluginShared<'_> for Shared {}

pub struct MainThread<'a> {
	host: HostMainThreadHandle<'a>,
	shared: &'a Shared,
}

impl<'a> PluginMainThread<'a, Shared> for MainThread<'a> {}

impl PluginAudioPortsImpl for MainThread<'_> {
	fn count(&mut self, _is_input: bool) -> u32 {
		1
	}

	fn get(&mut self, index: u32, _is_input: bool, writer: &mut AudioPortInfoWriter<'_>) {
		if index == 0 {
			writer.set(&AudioPortInfo {
				id: ClapId::new(0),
				name: b"main",
				channel_count: 2,
				flags: AudioPortFlags::IS_MAIN,
				port_type: Some(AudioPortType::STEREO),
				in_place_pair: Some(ClapId::new(0)),
			});
		}
	}
}

impl PluginMainThreadParams for MainThread<'_> {
	fn count(&mut self) -> u32 {
		6
	}

	fn get_info(&mut self, param_index: u32, info: &mut ParamInfoWriter<'_>) {
		info.set(&match param_index {
			0 => ParamInfo {
				id: PARAM_RATIO,
				flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
				cookie: Cookie::empty(),
				name: b"ratio",
				module: b"",
				min_value: 1f64.cbrt(),
				max_value: 24f64.cbrt(),
				default_value: 6f64.cbrt(),
			},
			1 => ParamInfo {
				id: PARAM_THRESHOLD,
				flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
				cookie: Cookie::empty(),
				name: b"threshold",
				module: b"",
				min_value: -24.0,
				max_value: 0.0,
				default_value: -12.0,
			},
			2 => ParamInfo {
				id: PARAM_KNEE,
				flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
				cookie: Cookie::empty(),
				name: b"knee",
				module: b"",
				min_value: 0.0,
				max_value: 24.0,
				default_value: 6.0,
			},
			3 => ParamInfo {
				id: PARAM_ATTACK,
				flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
				cookie: Cookie::empty(),
				name: b"attack",
				module: b"",
				min_value: 1f64.cbrt(),
				max_value: 100f64.cbrt(),
				default_value: 20f64.cbrt(),
			},
			4 => ParamInfo {
				id: PARAM_RELEASE,
				flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
				cookie: Cookie::empty(),
				name: b"release",
				module: b"",
				min_value: 5f64.cbrt(),
				max_value: 500f64.cbrt(),
				default_value: 100f64.cbrt(),
			},
			5 => ParamInfo {
				id: PARAM_POSTGAIN,
				flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
				cookie: Cookie::empty(),
				name: b"postgain",
				module: b"",
				min_value: -12.0,
				max_value: 12.0,
				default_value: 0.0,
			},
			_ => return,
		});
	}

	fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
		match param_id {
			PARAM_RATIO => Some(self.shared.ratio.load_value().cbrt().into()),
			PARAM_THRESHOLD => Some(self.shared.threshold.load_value().into()),
			PARAM_KNEE => Some(self.shared.knee.load_value().into()),
			PARAM_ATTACK => Some(self.shared.attack.load_value().cbrt().into()),
			PARAM_RELEASE => Some(self.shared.release.load_value().cbrt().into()),
			PARAM_POSTGAIN => Some(amp_to_db(self.shared.postgain.load_value()).into()),
			_ => None,
		}
	}

	fn value_to_text(
		&mut self,
		param_id: ClapId,
		value: f64,
		writer: &mut ParamDisplayWriter<'_>,
	) -> std::fmt::Result {
		match param_id {
			PARAM_RATIO => write!(writer, "{:.1}", value.powi(3)),
			PARAM_THRESHOLD | PARAM_KNEE | PARAM_POSTGAIN => write!(writer, "{value:.1} dB"),
			PARAM_ATTACK | PARAM_RELEASE => write!(writer, "{:.1} ms", value.powi(3)),
			_ => Err(std::fmt::Error),
		}
	}

	fn flush(
		&mut self,
		input_parameter_changes: &InputEvents<'_>,
		_output_parameter_changes: &mut OutputEvents<'_>,
	) {
		self.shared.flush(input_parameter_changes);
	}

	fn text_to_value(&mut self, param_id: ClapId, text: &CStr) -> Option<f64> {
		let text = text.to_str().ok()?;

		match param_id {
			PARAM_RATIO => Some(text.trim().parse::<f64>().ok()?.cbrt()),
			PARAM_THRESHOLD | PARAM_KNEE | PARAM_POSTGAIN => text
				.trim()
				.split_at_checked(text.len() - 2)
				.filter(|(_, suffix)| suffix.eq_ignore_ascii_case("dB"))
				.map_or(text, |(prefix, _)| prefix)
				.trim()
				.parse::<f64>()
				.ok(),
			PARAM_ATTACK | PARAM_RELEASE => Some(
				text.trim()
					.split_at_checked(text.len() - 2)
					.filter(|(_, suffix)| suffix.eq_ignore_ascii_case("ms"))
					.map_or(text, |(prefix, _)| prefix)
					.trim()
					.parse::<f64>()
					.ok()?
					.cbrt(),
			),
			_ => None,
		}
	}
}

impl PluginStateImpl for MainThread<'_> {
	fn load(&mut self, input: &mut InputStream<'_>) -> Result<(), PluginError> {
		let mut buf = [0; 4];
		input.read_exact(&mut buf)?;
		self.shared.ratio.store_value(f32::from_ne_bytes(buf));
		input.read_exact(&mut buf)?;
		self.shared.threshold.store_value(f32::from_ne_bytes(buf));
		input.read_exact(&mut buf)?;
		self.shared.knee.store_value(f32::from_ne_bytes(buf));
		input.read_exact(&mut buf)?;
		self.shared.attack.store_value(f32::from_ne_bytes(buf));
		input.read_exact(&mut buf)?;
		self.shared.release.store_value(f32::from_ne_bytes(buf));
		input.read_exact(&mut buf)?;
		self.shared.postgain.store_value(f32::from_ne_bytes(buf));

		if let Some(params) = self.host.get_extension::<HostParams>() {
			params.rescan(&mut self.host, ParamRescanFlags::VALUES);
		}

		Ok(())
	}

	fn save(&mut self, output: &mut OutputStream<'_>) -> Result<(), PluginError> {
		output.write_all(&self.shared.ratio.load_value().to_ne_bytes())?;
		output.write_all(&self.shared.threshold.load_value().to_ne_bytes())?;
		output.write_all(&self.shared.knee.load_value().to_ne_bytes())?;
		output.write_all(&self.shared.attack.load_value().to_ne_bytes())?;
		output.write_all(&self.shared.release.load_value().to_ne_bytes())?;
		output.write_all(&self.shared.postgain.load_value().to_ne_bytes())?;

		Ok(())
	}
}
