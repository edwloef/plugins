use crate::{cooler::Cooler, dcc::Dcc, heater::Heater, whiteout::Whiteout};
use clack_plugin::{entry::prelude::*, prelude::*};
use std::ffi::CStr;

clack_export_entry!(Plugins);

pub struct Plugins(PluginFactoryWrapper<PluginFactory>);

impl Entry for Plugins {
	fn new(_bundle_path: Option<&CStr>) -> Result<Self, EntryLoadError> {
		Ok(Self(PluginFactoryWrapper::new(PluginFactory {
			cooler: Cooler::get_descriptor(),
			dcc: Dcc::get_descriptor(),
			heater: Heater::get_descriptor(),
			whiteout: Whiteout::get_descriptor(),
		})))
	}

	fn declare_factories<'a>(&'a self, builder: &mut EntryFactories<'a>) {
		builder.register_factory(&self.0);
	}
}

struct PluginFactory {
	cooler: PluginDescriptor,
	dcc: PluginDescriptor,
	heater: PluginDescriptor,
	whiteout: PluginDescriptor,
}

impl PluginFactoryImpl for PluginFactory {
	fn plugin_count(&self) -> u32 {
		4
	}

	fn plugin_descriptor(&self, index: u32) -> Option<&PluginDescriptor> {
		match index {
			0 => Some(&self.cooler),
			1 => Some(&self.dcc),
			2 => Some(&self.heater),
			3 => Some(&self.whiteout),
			_ => None,
		}
	}

	fn create_plugin<'a>(
		&'a self,
		host_info: HostInfo<'a>,
		plugin_id: &CStr,
	) -> Option<PluginInstance<'a>> {
		match plugin_id.to_str().ok()? {
			Cooler::ID => Some(PluginInstance::new::<Cooler>(
				host_info,
				&self.dcc,
				Cooler::new_shared,
				Cooler::new_main_thread,
			)),
			Dcc::ID => Some(PluginInstance::new::<Dcc>(
				host_info,
				&self.dcc,
				Dcc::new_shared,
				Dcc::new_main_thread,
			)),
			Heater::ID => Some(PluginInstance::new::<Heater>(
				host_info,
				&self.heater,
				Heater::new_shared,
				Heater::new_main_thread,
			)),
			Whiteout::ID => Some(PluginInstance::new::<Whiteout>(
				host_info,
				&self.whiteout,
				Whiteout::new_shared,
				Whiteout::new_main_thread,
			)),
			_ => None,
		}
	}
}
