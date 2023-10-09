use eframe::{egui::{Context, Visuals}, HardwareAcceleration, NativeOptions};
mod catppuccin;

#[derive(serde::Deserialize, serde::Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct WindowOptions {
	pub(crate) decorated: bool,
	pub(crate) transparent: bool,
	pub(crate) vsync: bool,
	pub(crate) hardware_acceleration: bool,
}

impl Default for WindowOptions {
	fn default() -> Self {
		Self {
			decorated: true,
			transparent: true,
			vsync: true,
			hardware_acceleration: true,
		}
	}
}

impl From<WindowOptions> for NativeOptions {
    fn from(val: WindowOptions) -> Self {
        NativeOptions {
            decorated: val.decorated,
            transparent: val.transparent,
            vsync: val.vsync,
            hardware_acceleration: if val.hardware_acceleration {HardwareAcceleration::Preferred} else {HardwareAcceleration::Off},
            ..Default::default()
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(untagged)]
pub enum Theme {
    Template(ThemeTemplate),
    ExplicitTheme(Box<Visuals>),
}

impl Default for Theme {
    fn default() -> Self {
        Self::Template(ThemeTemplate::Frappe)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Default, PartialEq, Eq)]
pub enum ThemeTemplate {
	Egui,
	Mocha,
	Macchiato,
	Latte,
	#[default]
	Frappe,
}

impl ThemeTemplate {
	pub fn apply(&self, ctx: &Context) {
		if self == &ThemeTemplate::Egui {
			return;
		}
		let theme = match self {
			ThemeTemplate::Mocha => catppuccin::MOCHA,
			ThemeTemplate::Macchiato => catppuccin::MACCHIATO,
			ThemeTemplate::Latte => catppuccin::LATTE,
			ThemeTemplate::Frappe => catppuccin::FRAPPE,
			ThemeTemplate::Egui => unreachable!(),
		};
		catppuccin::set_theme(ctx, theme);
	}
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub(crate) struct Config {
	pub theme: Theme,
	pub window: WindowOptions,
}
