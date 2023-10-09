use eframe::{
	egui::{self, ScrollArea, SidePanel, TopBottomPanel},
	epaint::{Color32, Vec2},
};
use tokio::runtime::Runtime;
use winter::{
	document::media::MaybeLoaded,
	state::{ChannelFromBytesError, Database, WChannel},
};

pub(crate) struct MainApp {
	pub(crate) database: Database,
	pub(crate) selection: Option<Selection>,
	pub(crate) add_channel_working: Option<AddChannel>,
}

pub(crate) struct Selection {
	pub(crate) channel_id: String,
	pub(crate) article: Option<String>,
}

#[derive(Default)]
pub(crate) struct AddChannel {
	pub url: String,
	pub fetch_progress: Option<MaybeLoaded<WChannel>>,
}

impl MainApp {
	pub(crate) fn from_db(database: Database) -> Self {
		Self {
			database,
			selection: None,
			add_channel_working: None,
		}
	}
	pub(crate) fn update(
		&mut self,
		ctx: &eframe::egui::Context,
		_frame: &mut eframe::Frame,
		rt: &Runtime,
	) {
		let _rt = rt.enter();

		let selected_channel = self.selection.as_ref().map(|sel| &sel.channel_id).cloned();
		let selected_article = self
			.selection
			.as_ref()
			.and_then(|sel| sel.article.as_ref())
			.cloned();

		TopBottomPanel::bottom("sidebars").show(ctx, |ui| {
			self.bottom_panel(ui);
		});
		let show_channels = matches!(self.selection, None);
		let show_articles = matches!(
			self.selection,
			Some(Selection {
				channel_id: _,
				article: None
			})
		);

		SidePanel::left("channels")
			.show_animated(ctx, show_channels, |ui| self.channels_panel(ui, rt));

		SidePanel::left("articles").show_animated(ctx, show_articles, |ui| {});

		SidePanel::right("new_subscription").show_animated(
			ctx,
			self.add_channel_working.is_some(),
			|ui| {
				self.add_new_channel_panel(ui, rt);
			},
		);
	}

	fn add_new_channel_panel(&mut self, ui: &mut egui::Ui, rt: &Runtime) {
		ui.set_min_size(Vec2::new(200.0, 0.0));
		if let Some(add_channel) = &mut self.add_channel_working {
			if let Some(fetch_progress) = &mut add_channel.fetch_progress {
				rt.block_on(fetch_progress.tick());
			}
			ui.text_edit_singleline(&mut add_channel.url);
			if ui.button("Try it").clicked() {
				add_channel.fetch_progress =
					Some(MaybeLoaded::NotStarted(add_channel.url.to_string()));
			}
			match &add_channel.fetch_progress {
				None => {}
				Some(MaybeLoaded::Done(_, Err(ChannelFromBytesError::HTMLWithLink(link)))) => {
					add_channel.url = link.clone();
					add_channel.fetch_progress = Some(MaybeLoaded::NotStarted(link.to_string()));
				}
				Some(MaybeLoaded::Done(_, Err(e))) => {
					ui.colored_label(Color32::RED, format!("{e:#?}"));
				}
				Some(MaybeLoaded::Working(_) | MaybeLoaded::NotStarted(_)) => {
					ui.label("...");
				}
				Some(MaybeLoaded::BadStatus(status)) => {
					ui.colored_label(Color32::RED, format!("Got bad status code {status}"));
				}
				Some(MaybeLoaded::Failed(_, e)) => {
					ui.colored_label(Color32::RED, format!("{e}"));
				}
				Some(MaybeLoaded::Done(_, Ok(channel))) => {
					ui.colored_label(
						Color32::GREEN,
						format!("OK! Got feed \"{}\".", channel.0.title()),
					);
					if ui.button("Commit").clicked() {
						rt.block_on(self.database.subscribe(&add_channel.url, &channel.0));
						self.add_channel_working = None;
					}
				}
			}
			if ui.button("Cancel").clicked() {
				self.add_channel_working = None;
			}
		}
	}

	fn channels_panel(
		&mut self,
		ui: &mut egui::Ui,
		rt: &Runtime,
	) -> egui::scroll_area::ScrollAreaOutput<()> {
		ScrollArea::new([false, true]).show(ui, |ui| {
			for (key, value) in rt.block_on(self.database.get_subscriptions()) {
				if ui.button(&value.title).clicked() {
					self.selection = Some(Selection {
						channel_id: key.clone(),
						article: None,
					});
				}
				ui.collapsing("Description", |ui| {
					ui.label(&value.description);
					if ui.button("Unsubscribe").clicked() {
						rt.block_on(self.database.unsubscribe(&key));
					}
				});
				ui.separator();
			}
		})
	}

	fn bottom_panel(&mut self, ui: &mut egui::Ui) {
		ui.horizontal(|ui| {
			if ui.button("Back").clicked() {
				match &mut self.selection {
					None => {}
					Some(Selection {
						channel_id: _,
						article,
					}) if article.is_some() => {
						*article = None;
					}
					Some(_) => {
						self.selection = None;
					}
				}
			}
			if ui.button("New Subscription").clicked() {
				self.add_channel_working = Some(AddChannel::default());
			}
		});
	}
}
