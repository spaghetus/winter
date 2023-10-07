use std::{
	collections::{BTreeMap, BTreeSet},
	path::PathBuf,
	sync::Arc,
};

use base64::{engine::GeneralPurpose, Engine};
use rss::Channel;
use tokio::{sync::RwLock, task::JoinHandle};

use self::inotify::inotify_loop;

mod inotify;

/// Database for the program, which uses the filesystem atomically to allow syncing with
/// naive file-based tools.
pub struct Database {
	src_dir: PathBuf,
	_task: JoinHandle<()>,
	read_articles_cache: Arc<RwLock<BTreeSet<String>>>,
	subscriptions_cache: Arc<RwLock<BTreeMap<String, Channel>>>,
	base64: GeneralPurpose,
}

impl Database {
	pub fn from_dir(src_dir: PathBuf) -> Database {
		let read_articles = Arc::new(RwLock::new(BTreeSet::new()));
		let subscriptions = Arc::new(RwLock::new(BTreeMap::new()));
		let base64 = base64::engine::general_purpose::GeneralPurpose::new(
			&base64::alphabet::STANDARD,
			Default::default(),
		);

		let task = tokio::spawn({
			let subscriptions = subscriptions.clone();
			let read_articles = read_articles.clone();
			let src_dir = src_dir.clone();
			inotify_loop(src_dir.clone(), read_articles, subscriptions)
		});

		Database {
			src_dir,
			_task: task,
			read_articles_cache: read_articles,
			subscriptions_cache: subscriptions,
			base64,
		}
	}

	pub fn read(&self, article_guid: &str) {
		let name = {
			let mut name = String::new();
			self.base64.encode_string(&article_guid, &mut name);
			name
		};
		let path = self.src_dir.join(name);
		std::fs::write(path, []).expect("Failed to write marker file");
		self.read_articles_cache
			.blocking_write()
			.insert(article_guid.to_string());
	}

	pub fn unread(&self, article_guid: &str) {
		let deleted = self
			.read_articles_cache
			.blocking_write()
			.remove(article_guid);
		if deleted {
			let name = {
				let mut name = String::new();
				self.base64.encode_string(&article_guid, &mut name);
				name
			};
			let path = self.src_dir.join(name);
			std::fs::remove_file(path).expect("Failed to delete marker file");
		}
	}

	pub fn subscribe(&self, pub_url: &str, channel: Channel) {
		let mut subscriptions = self.subscriptions_cache.blocking_write();
		let sub = subscriptions.entry(pub_url.to_string()).or_default();
		sub.merge(&channel);
		let name = {
			let mut name = String::new();
			self.base64.encode_string(&pub_url, &mut name);
			name
		};
		let path = self.src_dir.join(name);
		let writer = std::fs::OpenOptions::new()
			.truncate(true)
			.write(true)
			.open(path)
			.expect("Failed to open subscription for writing");
		sub.write_to(writer).expect("Failed to write subscription");
	}

	pub fn unsubscribe(&self, pub_url: &str) {
		let mut subscriptions = self.subscriptions_cache.blocking_write();
		let deleted = subscriptions.remove(pub_url).is_some();
		if deleted {
			let name = {
				let mut name = String::new();
				self.base64.encode_string(&pub_url, &mut name);
				name
			};
			let path = self.src_dir.join(name);
			std::fs::remove_file(path).expect("Failed to delete subscription");
		}
	}
}

pub trait Merge {
	fn merge(&mut self, from: &Self);
}

impl Merge for Channel {
	fn merge(&mut self, from: &Self) {
		let guids_to_write: Vec<_> = from.items.iter().flat_map(|item| item.guid()).collect();
		let orig_items = self.items.clone();
		let mut new_items = from.items.clone();
		*self = Channel {
			items: orig_items,
			..from.clone()
		};
		self.items.retain(|item| {
			item.guid()
				.map(|g| guids_to_write.contains(&g))
				.unwrap_or(false)
		});
		self.items.append(&mut new_items)
	}
}
