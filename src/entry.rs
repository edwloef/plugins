use crate::{dcc::Dcc, whiteout::Whiteout};
use clack_plugin::{entry::prelude::*, prelude::*};
use std::ffi::CStr;

clack_export_entry!(Plugins);

pub struct Plugins(PluginFactoryWrapper<PluginFactory>);

impl Entry for Plugins {
	fn new(_bundle_path: Option<&CStr>) -> Result<Self, EntryLoadError> {
		Ok(Plugins(PluginFactoryWrapper::new(PluginFactory {
			dcc: Dcc::get_descriptor(),
			whiteout: Whiteout::get_descriptor(),
		})))
	}

	fn declare_factories<'a>(&'a self, builder: &mut EntryFactories<'a>) {
		builder.register_factory(&self.0);
	}
}

struct PluginFactory {
	dcc: PluginDescriptor,
	whiteout: PluginDescriptor,
}

impl PluginFactoryImpl for PluginFactory {
	fn plugin_count(&self) -> u32 {
		2
	}

	fn plugin_descriptor(&self, index: u32) -> Option<&PluginDescriptor> {
		match index {
			0 => Some(&self.dcc),
			1 => Some(&self.whiteout),
			_ => None,
		}
	}

	fn create_plugin<'a>(
		&'a self,
		host_info: HostInfo<'a>,
		plugin_id: &CStr,
	) -> Option<PluginInstance<'a>> {
		match plugin_id.to_str().ok()? {
			Dcc::ID => Some(PluginInstance::new::<Dcc>(
				host_info,
				&self.dcc,
				Dcc::new_shared,
				Dcc::new_main_thread,
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
