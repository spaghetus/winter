use crate::FETCHER;
use eframe::Frame;
#[cfg(feature = "gui")]
use eframe::egui::{Image, RichText};
use html_parser::{Dom, DomVariant, Element, Node};
use tokio::runtime::Runtime;

use self::media::{Audio, MaybeLoaded, Video};

pub mod media;

pub enum DocumentNode {
	Root(Vec<DocumentNode>),
	Div(Vec<DocumentNode>),
	Span(Vec<DocumentNode>),
	Unk(Vec<DocumentNode>),
	UList(Vec<DocumentNode>),
	OList(Vec<DocumentNode>),
	Emph(Vec<DocumentNode>),
	Strong(Vec<DocumentNode>),
	Sep,
	TextLeaf(String),
	Link {
		url: String,
		mime: String,
		label: Vec<DocumentNode>,
	},
	Image {
		label: String,
		url: String,
	},
	Video {
		label: String,
		fetched: MaybeLoaded<Video>,
	},
	Audio {
		label: String,
		fetched: MaybeLoaded<Audio>,
	},
	Empty,
}

macro_rules! from_iter {
	($iter:expr) => {
		$iter.into_iter().map(DocumentNode::from).collect()
	};
}

impl From<Dom> for DocumentNode {
	fn from(value: Dom) -> Self {
		if matches!(value.tree_type, DomVariant::Empty) || value.children.is_empty() {
			return Self::Empty;
		}

		Self::Root(value.children.into_iter().map(Self::from).collect())
	}
}

impl From<Node> for DocumentNode {
	fn from(value: Node) -> Self {
		match value {
			Node::Text(s) => Self::TextLeaf(s),
			Node::Element(e) => Self::from(e),
			Node::Comment(_) => Self::Empty,
		}
	}
}

impl From<Element> for DocumentNode {
	fn from(value: Element) -> Self {
		match value.name.as_str() {
			"div" => Self::Div(from_iter!(value.children)),
			"span" => Self::Span(from_iter!(value.children)),
			"ul" => Self::UList(from_iter!(value.children)),
			"ol" => Self::OList(from_iter!(value.children)),
			"hr" => Self::Sep,
			"a" => match value.attributes.get("type").cloned().flatten() {
				Some(mime) if mime.starts_with("image/") => Self::Image {
					label: "Linked image".to_string(),
					url: value
						.attributes
						.get("href")
						.cloned()
						.flatten()
						.unwrap_or_else(|| "about:blank".to_string()),
				},
				Some(mime) if mime.starts_with("audio/") => Self::Audio {
					label: "Linked audio file".to_string(),
					fetched: MaybeLoaded::NotStarted(value
						.attributes
						.get("href")
						.cloned()
						.flatten()
						.unwrap_or_else(|| "about:blank".to_string())),
				},
				Some(mime) if mime.starts_with("video/") => Self::Video {
					label: "Linked video file".to_string(),
					fetched: MaybeLoaded::NotStarted(value
						.attributes
						.get("href")
						.cloned()
						.flatten()
						.unwrap_or_else(|| "about:blank".to_string())),
				},
				other => Self::Link {
					url: value
						.attributes
						.get("href")
						.cloned()
						.flatten()
						.unwrap_or_else(|| "about:blank".to_string()),
					mime: other.unwrap_or_else(|| "text/plain".to_string()),
					label: from_iter!(value.children),
				},
			},
			"img" => Self::Image {
				label: value
					.attributes
					.get("alt")
					.cloned()
					.flatten()
					.unwrap_or_else(|| "No alt text".to_string()),
				url: value
					.attributes
					.get("src")
					.cloned()
					.flatten()
					.unwrap_or_else(|| "No alt text".to_string()),
			},
			"i" | "em" => Self::Emph(from_iter!(value.children)),
			"b" | "strong" => Self::Strong(from_iter!(value.children)),
			_ => Self::Unk(from_iter!(value.children)),
		}
	}
}

#[cfg(feature = "gui")]
#[allow(clippy::too_many_lines)]
impl DocumentNode {
	pub fn show(&mut self, ui: &mut eframe::egui::Ui, frame: &mut Frame) {
		match self {
			DocumentNode::Sep => {
				ui.separator();
			}
			DocumentNode::Root(inner) => {
				ui.vertical(|ui| {
					inner.iter_mut().for_each(|el| el.show(ui, frame));
				});
			}
			DocumentNode::Div(inner) => {
				ui.horizontal_wrapped(|ui| inner.iter_mut().for_each(|el| el.show(ui, frame)));
			}
			DocumentNode::Span(inner) | DocumentNode::Unk(inner) => {
				inner.iter_mut().for_each(|el| el.show(ui, frame));
			}
			DocumentNode::UList(inner) => {
				for el in inner.iter_mut() {
					ui.horizontal(|ui| {
						ui.label("* ");
						el.show(ui, frame);
					});
				}
			}
			DocumentNode::OList(inner) => {
				for (n, el) in inner.iter_mut().enumerate() {
					ui.horizontal(|ui| {
						ui.label(format!("{n}. "));
						el.show(ui, frame);
					});
				}
			}
			DocumentNode::Emph(inner) => {
				let orig_state =
					ui.memory(|memory| memory.data.get_temp("italic".into()).unwrap_or(false));
				ui.memory_mut(|memory| *memory.data.get_temp_mut_or("italic".into(), true) = true);
				inner.iter_mut().for_each(|el| el.show(ui, frame));
				ui.memory_mut(|memory| {
					*memory.data.get_temp_mut_or("italic".into(), orig_state) = orig_state;
				});
				ui.label("/");
			}
			DocumentNode::Strong(inner) => {
				let orig_state =
					ui.memory(|memory| memory.data.get_temp("strong".into()).unwrap_or(false));
				ui.memory_mut(|memory| *memory.data.get_temp_mut_or("strong".into(), true) = true);
				inner.iter_mut().for_each(|el| el.show(ui, frame));
				ui.memory_mut(|memory| {
					*memory.data.get_temp_mut_or("strong".into(), orig_state) = orig_state;
				});
				ui.label("/");
			}
			DocumentNode::TextLeaf(text) => {
				let strong =
					ui.memory(|memory| memory.data.get_temp("strong".into()).unwrap_or(false));
				let emph = ui.memory(|memory| memory.data.get_temp("emph".into()).unwrap_or(false));
				let mut text = RichText::new(text.clone());
				if strong {
					text = text.strong();
				}
				if emph {
					text = text.italics();
				}
				ui.label(text);
			}
			DocumentNode::Link { url, mime, label } => {
				let strong =
					ui.memory(|memory| memory.data.get_temp("strong".into()).unwrap_or(false));
				let emph = ui.memory(|memory| memory.data.get_temp("emph".into()).unwrap_or(false));
				let mut text = RichText::new(DocumentNode::many_to_string(label.iter(), " "));
				if strong {
					text = text.strong();
				}
				if emph {
					text = text.italics();
				}
				let button = ui.button(text);
				if button.clicked_by(eframe::egui::PointerButton::Middle) {
					open::that(url).expect("Failed to open that url");
				} else if button.clicked() {
					frame.set_minimized(true);
					open::that(url).expect("Failed to open that url");
				}
			}
			DocumentNode::Image { label, url } => {
				ui.label(label.as_str());
				ui.add(Image::new(url.as_str()).max_height(300.0));
			}
			DocumentNode::Video { label, fetched } => {
				ui.label(label.as_str());
				match fetched {
					MaybeLoaded::Done(_, Ok(media)) => media.show(ui),
					MaybeLoaded::Done(_, Err(e)) => {
						ui.label(format!("Error: {e}"));
					}
					_ => {
						ui.label("Loading video...");
					}
				}
			}
			DocumentNode::Audio { label, fetched } => {
				ui.label(label.as_str());
				match fetched {
					MaybeLoaded::Done(_, Ok(media)) => media.show(ui),
					MaybeLoaded::Done(_, Err(e)) => {
						ui.label(format!("Error: {e}"));
					}
					_ => {
						ui.label("Loading audio...");
					}
				}
			}
			DocumentNode::Empty => {}
		}
	}
}

impl DocumentNode {
	pub fn many_to_string<'a>(iter: impl Iterator<Item = &'a Self>, join: &str) -> String {
		iter.map(ToString::to_string).collect::<Vec<_>>().join(join)
	}

	pub fn tick(&mut self, rt: &Runtime) {
		match self {
			DocumentNode::Root(inner)
			| DocumentNode::Div(inner)
			| DocumentNode::Span(inner)
			| DocumentNode::Unk(inner)
			| DocumentNode::UList(inner)
			| DocumentNode::OList(inner)
			| DocumentNode::Emph(inner)
			| DocumentNode::Strong(inner) => {
				for child in inner {
					child.tick(rt);
				}
			}
			DocumentNode::Video { label: _, fetched } => {
				rt.block_on(fetched.tick());
			}
			DocumentNode::Audio { label: _, fetched } => {
				rt.block_on(fetched.tick());
			}
			_ => {}
		}
	}
}

impl ToString for DocumentNode {
	fn to_string(&self) -> String {
		match self {
			DocumentNode::TextLeaf(text) => text.to_string(),
			DocumentNode::Root(inner) => Self::many_to_string(inner.iter(), "\n"),
			DocumentNode::Div(inner)
			| DocumentNode::Span(inner)
			| DocumentNode::Emph(inner)
			| DocumentNode::Strong(inner) => Self::many_to_string(inner.iter(), " "),
			DocumentNode::Link { url: _, mime: _, label } => Self::many_to_string(label.iter(), " "),
			DocumentNode::Image { label, url: _ }
			| DocumentNode::Video { label, fetched: _ }
			| DocumentNode::Audio { label, fetched: _ } => label.to_string(),
			_ => "???".to_string(),
		}
	}
}
