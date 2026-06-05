use crate::{Param, amp_to_db, db_to_amp};
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
	plugin::features::{AUDIO_EFFECT, DISTORTION, STEREO},
	prelude::*,
	stream::{InputStream, OutputStream},
	utils::Cookie,
};
use std::{
	ffi::CStr,
	fmt::Write as _,
	io::{Read as _, Write as _},
};

pub struct Heater;

impl Heater {
	pub const ID: &'static str = "com.edwloef.heater";
}

impl Plugin for Heater {
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

impl DefaultPluginFactory for Heater {
	fn get_descriptor() -> PluginDescriptor {
		PluginDescriptor::new(Self::ID, "Heater")
			.with_version(env!("CARGO_PKG_VERSION"))
			.with_vendor("edwloef")
			.with_features([AUDIO_EFFECT, DISTORTION, STEREO])
	}

	fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
		Ok(Shared {
			pregain: Param::new(1.0),
			intensity: Param::new(0.0),
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
}

impl<'a> PluginAudioProcessor<'a, Shared, MainThread<'a>> for AudioProcessor<'a> {
	fn activate(
		_host: HostAudioProcessorHandle<'_>,
		_main_thread: &mut MainThread<'_>,
		shared: &'a Shared,
		_audio_config: PluginAudioConfiguration,
	) -> Result<Self, PluginError> {
		Ok(Self { shared })
	}

	fn process(
		&mut self,
		_process: Process<'_>,
		mut audio: Audio<'_>,
		events: Events<'_>,
	) -> Result<ProcessStatus, PluginError> {
		let mut channels = audio
			.port_pair(0)
			.ok_or(PluginError::Message("No audio ports found"))?
			.channels()?
			.into_f32()
			.ok_or(PluginError::Message("No f32 channels provided"))?;

		for batch in events.input.batch() {
			self.shared.flush(batch.events());

			let pregain = self.shared.pregain.load_combined();
			let intensity = self.shared.intensity.load_combined();
			let postgain = self.shared.postgain.load_combined();

			let process = |input: f32| {
				(input * pregain) / (intensity * ((input * pregain).abs() - 1.0) + 1.0) * postgain
			};

			for channel in channels.iter_mut() {
				match channel {
					ChannelPair::InputOnly(_) => {
						return Err(PluginError::Message("No output channel provided"));
					}
					ChannelPair::OutputOnly(_) => {
						return Err(PluginError::Message("No input channel provided"));
					}
					ChannelPair::InPlace(in_place) => {
						for in_place in &mut in_place[batch.sample_bounds()] {
							*in_place = process(*in_place);
						}
					}
					ChannelPair::InputOutput(input, output) => {
						for (input, output) in input[batch.sample_bounds()]
							.iter()
							.zip(&mut output[batch.sample_bounds()])
						{
							*output = process(*input);
						}
					}
				}
			}
		}

		Ok(ProcessStatus::ContinueIfNotQuiet)
	}
}

const PARAM_PREGAIN: ClapId = ClapId::new(0);
const PARAM_INTENSITY: ClapId = ClapId::new(1);
const PARAM_POSTGAIN: ClapId = ClapId::new(2);

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
	pregain: Param,
	intensity: Param,
	postgain: Param,
}

impl Shared {
	fn flush<'a>(&self, input_parameter_changes: impl IntoIterator<Item = &'a UnknownEvent>) {
		for event in input_parameter_changes {
			match event.as_core_event() {
				Some(CoreEventSpace::ParamValue(event)) => match event.param_id() {
					Some(PARAM_PREGAIN) => {
						self.pregain.store_value(db_to_amp(event.value() as f32));
					}
					Some(PARAM_INTENSITY) => self.intensity.store_value(event.value() as f32),
					Some(PARAM_POSTGAIN) => {
						self.postgain.store_value(db_to_amp(event.value() as f32));
					}
					_ => {}
				},
				Some(CoreEventSpace::ParamMod(event)) => match event.param_id() {
					Some(PARAM_PREGAIN) => self.pregain.store_mod(db_to_amp(event.amount() as f32)),
					Some(PARAM_INTENSITY) => self.intensity.store_mod(event.amount() as f32),
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
		3
	}

	fn get_info(&mut self, param_index: u32, info: &mut ParamInfoWriter<'_>) {
		info.set(&match param_index {
			0 => ParamInfo {
				id: PARAM_PREGAIN,
				flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
				cookie: Cookie::empty(),
				name: b"pregain",
				module: b"",
				min_value: -12.0,
				max_value: 12.0,
				default_value: 0.0,
			},
			1 => ParamInfo {
				id: PARAM_INTENSITY,
				flags: ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE,
				cookie: Cookie::empty(),
				name: b"intensity",
				module: b"",
				min_value: 0.0,
				max_value: 1f32.next_down().into(),
				default_value: 0.0,
			},
			2 => ParamInfo {
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
			PARAM_PREGAIN => Some(amp_to_db(self.shared.pregain.load_value()).into()),
			PARAM_INTENSITY => Some(self.shared.intensity.load_value().into()),
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
			PARAM_PREGAIN | PARAM_POSTGAIN => write!(writer, "{value:.1} dB"),
			PARAM_INTENSITY => write!(writer, "{}%", (value * 100.0).round() as i8),
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
			PARAM_PREGAIN | PARAM_POSTGAIN => text
				.trim()
				.split_at_checked(text.len() - 2)
				.filter(|(_, suffix)| suffix.eq_ignore_ascii_case("dB"))
				.map_or(text, |(prefix, _)| prefix)
				.trim()
				.parse::<f64>()
				.ok(),
			PARAM_INTENSITY => Some(
				text.trim()
					.strip_suffix("%")
					.unwrap_or(text)
					.trim()
					.parse::<f64>()
					.ok()? / 100.0,
			),
			_ => None,
		}
	}
}

impl PluginStateImpl for MainThread<'_> {
	fn load(&mut self, input: &mut InputStream<'_>) -> Result<(), PluginError> {
		let mut buf = [0; 4];
		input.read_exact(&mut buf)?;
		self.shared.pregain.store_value(f32::from_ne_bytes(buf));
		input.read_exact(&mut buf)?;
		self.shared.intensity.store_value(f32::from_ne_bytes(buf));
		input.read_exact(&mut buf)?;
		self.shared.postgain.store_value(f32::from_ne_bytes(buf));

		if let Some(params) = self.host.get_extension::<HostParams>() {
			params.rescan(&mut self.host, ParamRescanFlags::VALUES);
		}

		Ok(())
	}

	fn save(&mut self, output: &mut OutputStream<'_>) -> Result<(), PluginError> {
		output.write_all(&self.shared.pregain.load_value().to_ne_bytes())?;
		output.write_all(&self.shared.intensity.load_value().to_ne_bytes())?;
		output.write_all(&self.shared.postgain.load_value().to_ne_bytes())?;

		Ok(())
	}
}
