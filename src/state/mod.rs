use std::{
	collections::{BTreeMap, BTreeSet},
	fmt::{Debug, Display},
	path::PathBuf,
	sync::Arc, string::FromUtf8Error, io::BufReader,
};

use base64::{
	engine::{GeneralPurpose, GeneralPurposeConfig},
	Engine,
};
use rss::Channel;
use thiserror::Error;
use tokio::{sync::RwLock, task::JoinHandle};

use crate::feed::find_feed;

use self::inotify::inotify_loop;

mod inotify;

/// Database for the program, which uses the filesystem atomically to allow syncing with
/// naive file-based tools.
pub struct Database {
	src_dir: PathBuf,
	read_dir: PathBuf,
	subs_dir: PathBuf,
	_task: JoinHandle<()>,
	read_articles_cache: Arc<RwLock<BTreeSet<String>>>,
	subscriptions_cache: Arc<RwLock<BTreeMap<String, Arc<Channel>>>>,
	base64: GeneralPurpose,
}

impl Debug for Database {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Database")
			.field("src_dir", &self.src_dir)
			.field("read_articles_cache", &self.read_articles_cache)
			.field("subscriptions_cache", &self.subscriptions_cache)
			.finish()
	}
}

impl Database {
	#[must_use]
	pub fn from_dir(src_dir: PathBuf) -> Database {
		let read_articles = Arc::new(RwLock::new(BTreeSet::new()));
		let subscriptions = Arc::new(RwLock::new(BTreeMap::new()));
		let base64 = base64::engine::general_purpose::GeneralPurpose::new(
			&base64::alphabet::STANDARD,
			GeneralPurposeConfig::default(),
		);
		let read_dir = src_dir.join("read");
		let subs_dir = src_dir.join("subs");
		std::fs::create_dir_all(&read_dir).expect("Couldn't make read dir");
		std::fs::create_dir_all(&subs_dir).expect("Couldn't make subs dir");

		let task = tokio::spawn({
			let subscriptions = subscriptions.clone();
			let read_articles = read_articles.clone();
			let src_dir = src_dir.clone();
			inotify_loop(src_dir.clone(), read_articles, subscriptions)
		});

		Database {
			src_dir,
			read_dir,
			subs_dir,
			_task: task,
			read_articles_cache: read_articles,
			subscriptions_cache: subscriptions,
			base64,
		}
	}

	pub async fn read(&self, pub_url: &str, article_guid: &str) {
		let article_guid = format!("{pub_url}%{article_guid}");
		let name = {
			let mut name = String::new();
			self.base64.encode_string(&article_guid, &mut name);
			name
		};
		self.read_articles_cache.write().await.insert(article_guid);
		let path = self.read_dir.join(name);
		tokio::fs::write(path, r"This article has been read")
			.await
			.expect("Failed to write marker file");
	}

	pub async fn unread(&self, pub_url: &str, article_guid: &str) {
		let article_guid = format!("{pub_url}%{article_guid}");
		let deleted = self.read_articles_cache.write().await.remove(&article_guid);
		if deleted {
			let name = {
				let mut name = String::new();
				self.base64.encode_string(article_guid, &mut name);
				name
			};
			let path = self.read_dir.join(name);
			tokio::fs::remove_file(path)
				.await
				.expect("Failed to delete marker file");
		}
	}

	#[must_use]
	pub async fn has_read(&self, pub_url: &str, article_guid: &str) -> bool {
		let article_guid = format!("{pub_url}%{article_guid}");
		self.read_articles_cache
			.read()
			.await
			.contains(&article_guid)
	}

	pub async fn subscribe(&self, pub_url: &str, channel: &Channel) {
		let sub = {
			let mut subscriptions = self.subscriptions_cache.write().await;
			let mut sub: Channel = subscriptions
				.get(pub_url)
				.map(|a| a.as_ref().clone())
				.unwrap_or_default();
			sub.merge(channel);
			let sub = Arc::new(sub);
			subscriptions.insert(pub_url.to_string(), sub.clone());
			sub
		};
		let name = {
			let mut name = String::new();
			self.base64.encode_string(pub_url, &mut name);
			name
		};
		let path = self.subs_dir.join(name);
		let writer = tokio::fs::OpenOptions::new()
			.create(true)
			.truncate(true)
			.write(true)
			.open(path)
			.await
			.expect("Failed to open subscription for writing");
		sub.write_to(writer.into_std().await)
			.expect("Failed to write subscription");
	}

	pub async fn unsubscribe(&self, pub_url: &str) {
		let mut subscriptions = self.subscriptions_cache.write().await;
		let deleted = subscriptions.remove(pub_url).is_some();
		if deleted {
			let name = {
				let mut name = String::new();
				self.base64.encode_string(pub_url, &mut name);
				name
			};
			let path = self.subs_dir.join(name);
			tokio::fs::remove_file(path)
				.await
				.expect("Failed to delete subscription");
		}
	}

	pub async fn get_subscriptions(&self) -> BTreeMap<String, Arc<Channel>> {
		self.subscriptions_cache
			.read()
			.await
			.iter()
			.map(|(pub_url, channel)| (pub_url.clone(), channel.clone()))
			.collect()
	}

	pub async fn get_subscription(&self, pub_url: &str) -> Option<Arc<Channel>> {
		self.subscriptions_cache.read().await.get(pub_url).cloned()
	}
}

pub trait Merge {
	fn merge(&mut self, from: &Self);
}

impl Merge for Channel {
	fn merge(&mut self, from: &Self) {
		let guids_to_write: Vec<_> = from.items.iter().filter_map(rss::Item::guid).collect();
		let orig_items = self.items.clone();
		let mut new_items = from.items.clone();
		*self = Channel {
			items: orig_items,
			..from.clone()
		};
		self.items
			.retain(|item| item.guid().map_or(false, |g| guids_to_write.contains(&g)));
		self.items.append(&mut new_items);
	}
}

#[derive(Error, Debug)]
pub enum ChannelFromBytesError {
	BadRSS(#[from] rss::Error),
	BadUTF8(#[from] FromUtf8Error),
	HTMLWithLink(String),
}

impl Display for ChannelFromBytesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

pub struct WChannel(pub Channel);

impl TryFrom<Vec<u8>> for WChannel {
    type Error = ChannelFromBytesError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		let channel = match Channel::read_from(value.as_slice()) {
			Ok(c) => c,
			Err(e) => {
				let text = String::from_utf8(value)?;
				if let Some(link) = find_feed(&text).first() {
					Err(ChannelFromBytesError::HTMLWithLink(link.to_string()))?
				} else {
					Err(e)?
				}
			}
		};
		Ok(Self(channel))
    }
}


#[cfg(test)]
mod test {
	use super::Database;
	use rss::Channel;
	use std::time::Duration;

	#[tokio::test]
	async fn local_usage() {
		let tmp = tempdir::TempDir::new("winter_db_test").unwrap();
		dbg!(&tmp);
		let db = Database::from_dir(tmp.path().to_path_buf());
		db.read("TestUrl", "TestArticle").await;
		db.subscribe("TestUrl", &Channel::default()).await;
		assert!(db.has_read("TestUrl", "TestArticle").await);
		assert!(db.get_subscription("TestUrl").await.is_some());
		tokio::time::sleep(Duration::from_secs(2)).await;
		dbg!(&db);
		assert!(db.has_read("TestUrl", "TestArticle").await);
		assert!(db.get_subscription("TestUrl").await.is_some());
		std::mem::drop(db);
		std::mem::drop(tmp);
	}

	#[tokio::test]
	async fn foreign_usage() {
		let tmp = tempdir::TempDir::new("winter_db_test").unwrap();
		dbg!(&tmp);
		let db_a = Database::from_dir(tmp.path().to_path_buf());
		let db_b = Database::from_dir(tmp.path().to_path_buf());
		db_a.read("TestURL", "TestArticle").await;
		db_a.subscribe("TestUrl", &Channel::default()).await;
		for _ in 0..10 {
			tokio::time::sleep(Duration::from_secs(1)).await;
			if db_b.has_read("TestURL", "TestArticle").await
				&& db_b.get_subscription("TestUrl").await.is_some()
			{
				break;
			}
		}
		assert!(db_b.has_read("TestURL", "TestArticle").await);
		assert!(db_b.get_subscription("TestUrl").await.is_some());
		std::mem::drop(db_a);
		std::mem::drop(db_b);
		std::mem::drop(tmp);
	}
}
