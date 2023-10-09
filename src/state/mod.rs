use std::{
	collections::{BTreeMap, BTreeSet},
	fmt::{Debug, Display},
	io::BufReader,
	path::PathBuf,
	str::FromStr,
	string::FromUtf8Error,
	sync::Arc,
};

use base64::{
	engine::{GeneralPurpose, GeneralPurposeConfig},
	Engine,
};
use chrono::{DateTime, Local, NaiveDate};
use html_parser::Dom;
use rss::Channel;
use crate::syndication::Feed;
use thiserror::Error;
use tokio::{sync::RwLock, task::JoinHandle};

use crate::{document::DocumentNode, feed::find_feed};

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
	subscriptions_cache: Arc<RwLock<BTreeMap<String, Arc<Feed>>>>,
	base64: GeneralPurpose,
}

impl Debug for Database {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Database")
			.field("src_dir", &self.src_dir)
			.field("read_articles_cache", &self.read_articles_cache)
			.field(
				"subscriptions_cache",
				&self
					.subscriptions_cache
					.blocking_read()
					.iter()
					.map(|(k, v)| (k, v.to_string()))
					.collect::<BTreeMap<_, _>>(),
			)
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

	pub async fn subscribe(&self, pub_url: &str, channel: &Feed) {
		let sub = {
			let mut subscriptions = self.subscriptions_cache.write().await;
			let mut sub: Feed = subscriptions.get(pub_url).map_or(
				match channel {
					Feed::Atom(_) => Feed::Atom(atom_syndication::Feed::default()),
					Feed::RSS(_) => Feed::RSS(Channel::default()),
				},
				|a| a.as_ref().clone(),
			);
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
		tokio::fs::write(path, sub.to_string())
			.await
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

	pub async fn get_subscriptions(&self) -> BTreeMap<String, Arc<Feed>> {
		self.subscriptions_cache
			.read()
			.await
			.iter()
			.map(|(pub_url, channel)| (pub_url.clone(), channel.clone()))
			.collect()
	}

	pub async fn get_subscription(&self, pub_url: &str) -> Option<Arc<Feed>> {
		self.subscriptions_cache.read().await.get(pub_url).cloned()
	}
}

pub trait Merge {
	fn merge(&mut self, from: &Self);
}

impl Merge for Feed {
	fn merge(&mut self, from: &Self) {
		match (self, from) {
			(Feed::Atom(l), Feed::Atom(r)) => l.merge(r),
			(Feed::RSS(l), Feed::RSS(r)) => l.merge(r),
			_ => {
				eprintln!("Mismatched feeds!")
			}
		}
	}
}

impl Merge for rss::Channel {
	fn merge(&mut self, from: &Self) {
		let guids_to_write: Vec<_> = from.items.iter().filter_map(rss::Item::guid).collect();
		let orig_items = self.items.clone();
		let mut new_items = from.items.clone();
		*self = rss::Channel {
			items: orig_items,
			..from.clone()
		};
		self.items
			.retain(|item| item.guid().map_or(false, |g| !guids_to_write.contains(&g)));
		self.items.append(&mut new_items);
	}
}

impl Merge for atom_syndication::Feed {
	fn merge(&mut self, from: &Self) {
		let guids_to_write: Vec<_> = from
			.entries()
			.iter()
			.map(atom_syndication::Entry::id)
			.collect();
		let mut orig_items = self.entries().to_vec();
		let mut new_items = from.entries().to_vec();
		orig_items.retain(|item| !guids_to_write.contains(&item.id()));
		orig_items.append(&mut new_items);
		let mut new = from.clone();
		new.set_entries(orig_items);
		*self = new;
	}
}
pub struct CommonArticle {
	pub pub_url: String,
	pub id: String,
	pub title: String,
	pub authors: Vec<(String, Option<String>)>,
	pub categories: Vec<String>,
	pub body: Box<dyn Fn() -> DocumentNode>,
	pub links: Vec<(String, String, String)>,
	pub timestamp: DateTime<Local>,
}

impl CommonArticle {
	#[must_use]
	#[allow(clippy::too_many_lines)]
	pub fn from_feed(feed: &Feed, url: String) -> Vec<Self> {
		match &feed {
			Feed::Atom(a) => a
				.entries()
				.iter()
				.map(|entry| CommonArticle {
					pub_url: url.clone(),
					timestamp: entry.updated().with_timezone(&Local),
					id: entry.id().to_string(),
					title: entry.title().to_string(),
					authors: entry
						.authors()
						.iter()
						.map(|person| {
							(
								person.name().to_string(),
								person.email().map(ToString::to_string),
							)
						})
						.collect(),
					categories: entry
						.categories()
						.iter()
						.map(|cat| cat.term().to_string())
						.collect(),
					links: entry
						.links()
						.iter()
						.map(|link| {
							(
								link.title().unwrap_or("?").to_string(),
								link.mime_type().unwrap_or("text/plain").to_string(),
								link.href().to_string(),
							)
						})
						.collect(),
					body: {
						let content = entry
							.content()
							.and_then(atom_syndication::Content::value)
							.unwrap_or("<i>empty content</i>")
							.to_string();
						Box::new(move || {
							Dom::parse(&content)
								.unwrap_or(
									Dom::parse("<i>invalid dom</i>")
										.expect("default dom invalid?!"),
								)
								.into()
						})
					},
				})
				.collect(),
			Feed::RSS(r) => r
				.items()
				.iter()
				.map(|item| CommonArticle {
					pub_url: url.clone(),
					id: item.guid().map_or_else(
						|| {
							item.title
								.clone()
								.unwrap_or_else(|| "?".to_string())
								.to_string()
						},
						|g| g.value.clone(),
					),
					timestamp: item
						.pub_date()
						.and_then(|date| DateTime::parse_from_rfc2822(date).ok())
						.map_or(DateTime::from_timestamp(0, 0)
								.unwrap()
								.with_timezone(&Local), |d| d.with_timezone(&Local)),
					title: item.title.clone().unwrap_or_else(|| "?".to_string()),
					authors: item.author.clone().map(|a| (a, None)).into_iter().collect(),
					categories: item
						.categories()
						.iter()
						.map(|cat| cat.name.clone())
						.collect(),
					links: item
						.link()
						.map(|l| (l.to_string(), "text/plain".to_string(), l.to_string()))
						.into_iter()
						.collect(),
					body: {
						let content = item
							.content
							.clone()
    						.or_else(|| item.description.clone())
							.unwrap_or_else(|| "<i>empty content</i>".to_string());
						Box::new(move || {
							Dom::parse(&content)
								.unwrap_or(
									Dom::parse("<i>invalid dom</i>")
										.expect("default dom invalid?!"),
								)
								.into()
						})
					},
				})
				.collect(),
		}
	}
}

#[derive(Error, Debug)]
pub enum ChannelFromBytesError {
	BadFeed(&'static str),
	BadUTF8(#[from] FromUtf8Error),
	HTMLWithLink(String),
}

impl Display for ChannelFromBytesError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{self:?}")
	}
}

pub struct WFeed(pub Feed);

impl TryFrom<Vec<u8>> for WFeed {
	type Error = ChannelFromBytesError;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		let text = String::from_utf8(value)?;
		let channel = match Feed::from_str(&text) {
			Ok(c) => c,
			Err(e) => {
				if let Some(link) = find_feed(&text).first() {
					Err(ChannelFromBytesError::HTMLWithLink(link.to_string()))?
				} else {
					Err(ChannelFromBytesError::BadFeed(e))?
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
	use crate::syndication::Feed;

	#[tokio::test]
	async fn local_usage() {
		let tmp = tempdir::TempDir::new("winter_db_test").unwrap();
		let db = Database::from_dir(tmp.path().to_path_buf());
		db.read("TestUrl", "TestArticle").await;
		db.subscribe("TestUrl", &Feed::RSS(Default::default()))
			.await;
		assert!(db.has_read("TestUrl", "TestArticle").await);
		assert!(db.get_subscription("TestUrl").await.is_some());
		tokio::time::sleep(Duration::from_secs(2)).await;
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
		db_a.subscribe("TestUrl", &Feed::RSS(Channel::default()))
			.await;
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
