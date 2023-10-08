use std::sync::Arc;

use eframe::{egui::{self, ScrollArea}, NativeOptions};
use tokio::runtime::Runtime;
use winter::document::DocumentNode;

struct App(DocumentNode, Arc<Runtime>);

impl eframe::App for App {
	fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
		egui_extras::install_image_loaders(ctx);
		self.0.tick(&self.1);
		egui::CentralPanel::default().show(ctx, |ui| {
			ScrollArea::new([true, true]).show(ui, |ui| {
				ui.vertical(|ui| {
					self.0.show(ui);
				})
			});
		});
	}
}

fn main() {
	let rt = Runtime::new().expect("Failed to start runtime");
	let rt = Arc::new(rt);

	let app = App(
		DocumentNode::Root(vec![
			DocumentNode::Image {
				label: "Test Image".to_string(),
				url: "https://picsum.photos/200/300".to_string(),
			},
			DocumentNode::Audio {
				label: "Test Audio".to_string(),
				fetched: winter::document::media::MaybeLoaded::NotStarted(
					"https://download.samplelib.com/mp3/sample-3s.mp3".to_string(),
				),
			},
			DocumentNode::Video {
				label: "Test Video".to_string(),
				fetched: winter::document::media::MaybeLoaded::NotStarted(
					"https://download.samplelib.com/mp4/sample-5s.mp4".to_string(),
				),
			},
		]),
		rt.clone(),
	);

	eframe::run_native(
		"Test Document Tree",
		NativeOptions::default(),
		Box::new(move |_| Box::new(app)),
	)
	.expect("App crashed");
}
