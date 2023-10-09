use std::string::ToString;

use eframe::{
	egui::{self, CentralPanel, CollapsingHeader, ScrollArea, SidePanel, TopBottomPanel},
	epaint::{Color32, Vec2}, Frame,
};

use tokio::runtime::Runtime;
use winter::{
	document::{media::MaybeLoaded, DocumentNode},
	state::{ChannelFromBytesError, CommonArticle, Database, WFeed},
	syndication::Feed,
};

pub(crate) struct MainApp {
	pub(crate) database: Database,
	pub(crate) selection: Option<Selection>,
	pub(crate) add_channel_working: Option<AddChannel>,
}

pub(crate) struct Selection {
	pub(crate) channel_id: String,
	pub(crate) article: Option<SelectedArticle>,
}

pub(crate) struct SelectedArticle {
	article: CommonArticle,
	tree: DocumentNode,
	links: Vec<DocumentNode>,
}

impl SelectedArticle {
	pub fn populate_links(mut self, _rt: &Runtime) -> Self {
		self.links = self
			.article
			.links
			.iter()
			.map(|(label, mime, href)| {
				if href.starts_with("https://www.youtube.com/watch")
					|| href.starts_with("https://youtu.be")
				{
					return DocumentNode::Link {
						label: vec![DocumentNode::TextLeaf("YouTube Video".to_string())],
						mime: "text/html".to_string(),
						url: href.to_string(),
					};
				}
				DocumentNode::Link {
					url: href.clone(),
					mime: mime.clone(),
					label: vec![DocumentNode::TextLeaf(label.clone())],
				}
			})
			.collect();
		self
	}
}

#[derive(Default)]
pub(crate) struct AddChannel {
	pub url: String,
	pub fetch_progress: Option<MaybeLoaded<WFeed>>,
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
		frame: &mut eframe::Frame,
		rt: &Runtime,
	) {
		let _rt = rt.enter();

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

		SidePanel::left("articles").show_animated(ctx, show_articles, |ui| {
			self.articles_panel(ui, rt);
		});

		SidePanel::right("new_subscription").show_animated(
			ctx,
			self.add_channel_working.is_some(),
			|ui| {
				self.add_new_channel_panel(ui, rt);
			},
		);

		CentralPanel::default().show(ctx, |ui| {
			self.central_panel(ui, rt, frame);
		});
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
						format!(
							"OK! Got feed \"{}\".",
							match &channel.0 {
								Feed::RSS(r) => r.title().to_string(),
								Feed::Atom(a) => a.title().to_string(),
							}
						),
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
			ui.set_min_size(Vec2::new(200.0, 0.0));
			ScrollArea::new([false, true]).show(ui, |ui| {
				for (key, value) in rt.block_on(self.database.get_subscriptions()) {
					let title = match &*value {
						Feed::Atom(a) => a.title().to_string(),
						Feed::RSS(r) => r.title().to_string(),
					};
					let description = match &*value {
						Feed::Atom(_) => "Atom feed, no description available",
						Feed::RSS(r) => r.description(),
					};
					if ui.button(&title).clicked() {
						self.selection = Some(Selection {
							channel_id: key.clone(),
							article: None,
						});
					}
					CollapsingHeader::new("Description")
						.id_source(&title)
						.show(ui, |ui| {
							ui.label(description);
							if ui.button("Unsubscribe").clicked() {
								rt.block_on(self.database.unsubscribe(&key));
							}
						});
					ui.separator();
				}
			});
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

	fn articles_panel(&mut self, ui: &mut egui::Ui, rt: &Runtime) {
		ui.set_min_size(Vec2::new(200.0, 0.0));
		let Some(selection) = &mut self.selection else {
			return;
		};
		let Some(channel) = rt.block_on(self.database.get_subscription(&selection.channel_id)) else {
			self.selection = None;
			return;
		};
		let mut articles: Vec<CommonArticle> =
			CommonArticle::from_feed(&channel, selection.channel_id.clone());
		articles.sort_by_key(|article| article.timestamp);
		articles.reverse();
		ScrollArea::new([false, true]).show(ui, |ui| {
			for article in articles {
				ui.horizontal_wrapped(|ui| {
					if ui.button(&article.title).clicked() {
						let body = (article.body)();
						selection.article = Some(
							SelectedArticle {
								article,
								tree: body,
								links: vec![],
							}
							.populate_links(rt),
						);
						return;
					}
					ui.label(article.timestamp.date_naive().to_string());
					if rt.block_on(self.database.has_read(&selection.channel_id, &article.id)) {
						ui.label("(read)");
					}
				});
			}
		});
	}

	fn central_panel(&mut self, ui: &mut egui::Ui, rt: &Runtime, frame: &mut Frame) {
		let Some(Selection { channel_id, article: Some(SelectedArticle { article, tree, links }) }) = &mut self.selection else {
			ui.label("Select an article.");
			return;
		};
		tree.tick(rt);

		ui.heading(&article.title);
		ui.separator();
		ui.horizontal(|ui| {
			for (name, email) in &article.authors {
				if let Some(email) = email {
					if ui.button(name).clicked() {
						open::that(format!("mailto:{email}")).unwrap();
					}
				} else {
					ui.label(name);
				}
			}
		});
		ui.horizontal(|ui| ui.label(article.categories.join(", ")));

		for node in links {
			node.tick(rt);
			node.show(ui, frame);
		}

		ui.separator();

		tree.show(ui, frame);

		ui.separator();
		if ui.button("Mark as Read").clicked() {
			rt.block_on(self.database.read(channel_id, &article.id));
			self.selection = Some(Selection {
				channel_id: (*channel_id).to_string(),
				article: None,
			});
		}
	}
}
