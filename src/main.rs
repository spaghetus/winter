use clap::Parser;
use eframe::{
	egui::CentralPanel,
};
use figment::{
	providers::{Format, Serialized, Toml},
	Figment, Profile,
};
use gui_config::Theme;

use std::{path::PathBuf, sync::Arc};
use tokio::runtime::Runtime;
use winter::{
	state::Database, document::media::TMP,
};

struct App {
	inner: InnerApp,
	theme: Option<Theme>,
	rt: Arc<Runtime>,
}

enum InnerApp {
	PickDirectory(PickDirectoryApp),
	Working(main_app::MainApp),
}

impl eframe::App for App {
	fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
		if let Some(theme) = self.theme.take() {
			match theme {
				Theme::Template(template) => template.apply(ctx),
				Theme::ExplicitTheme(visuals) => ctx.set_visuals(*visuals),
			}
		}
		match &mut self.inner {
			InnerApp::PickDirectory(p) => {
				if let Some(new_app) = p.update(ctx, frame, &self.rt) {
					self.inner = InnerApp::Working(new_app);
				}
			}
			InnerApp::Working(m) => m.update(ctx, frame, &self.rt),
		}
	}
}

struct PickDirectoryApp;

impl PickDirectoryApp {
	#[allow(clippy::unused_self)]
	fn update(
		&mut self,
		ctx: &eframe::egui::Context,
		_frame: &mut eframe::Frame,
		rt: &Runtime,
	) -> Option<main_app::MainApp> {
		let _rt = rt.enter();
		let mut out = None;
		CentralPanel::default().show(ctx, |ui| {
			ui.vertical_centered(|ui| {
				ui.heading("Select a working directory");
				ui.label("(To avoid this step in the future, modify your application menu entry to include the target as a command-line argument)");
				ui.separator();
				if ui.button("Pick a Directory").clicked() {
					if let Some(chosen) = rfd::FileDialog::new().pick_folder() {
						let database = Database::from_dir(chosen);
						out = Some(main_app::MainApp::from_db(database));
					}
				}
			});
		});
		out
	}
}

mod main_app;

#[derive(clap::Parser)]
struct Args {
	/// The path to a configuration file. You can specify as many of these as you need to.
	#[arg(short, long)]
	config_path: Vec<PathBuf>,
	/// The path to the target directory. If this is unset, you will need to choose a directory at runtime.
	target_directory: Option<PathBuf>,
}

mod gui_config;

fn main() {
	// Parse arguments
	let args = Args::parse();
	// Load config
	let config = gui_config::Config::default();
	let mut config = Figment::new().merge(Serialized::from(config, Profile::Default));
	if let Ok(xdg) = xdg::BaseDirectories::new() {
		if let Some(location) = xdg.find_config_file("winter.toml") {
			config = config.merge(Toml::file(location));
		}
	}
	for location in args.config_path {
		config = config.merge(Toml::file(location));
	}
	let config: gui_config::Config = config.extract().expect("Invalid config");
	let rt = Arc::new(Runtime::new().expect("Init runtime"));
	// Build app
	let app = if let Some(target_dir) = args.target_directory {
		let _rt = rt.enter();
		InnerApp::Working(main_app::MainApp::from_db(Database::from_dir(target_dir)))
	} else {
		InnerApp::PickDirectory(PickDirectoryApp)
	};
	let app = App {
		inner: app,
		rt: rt.clone(),
		theme: Some(config.theme),
	};
	eframe::run_native(
		"winter",
		config.window.into(),
		Box::new(move |_| Box::new(app)),
	)
	.expect("App crashed");
	if let Some(tmp) = TMP.write().unwrap().take() {
		tmp.close().expect("Failed to destroy temporary files");
	}
}
