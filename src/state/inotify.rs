use std::{
	collections::{BTreeMap, BTreeSet},
	io::BufReader,
	path::{Path, PathBuf},
	sync::Arc,
	time::Duration,
};

use base64::{
	engine::{GeneralPurpose, GeneralPurposeConfig},
	Engine,
};
use inotify::{Inotify, WatchMask};
use rss::Channel;
use tokio::{fs::OpenOptions, sync::RwLock};

use super::Merge;

pub async fn inotify_loop(
	src_dir: PathBuf,
	read_articles: Arc<RwLock<BTreeSet<String>>>,
	subscriptions: Arc<RwLock<BTreeMap<String, Arc<Channel>>>>,
) {
	let base64 = base64::engine::general_purpose::GeneralPurpose::new(
		&base64::alphabet::STANDARD,
		GeneralPurposeConfig::default(),
	);
	let read_dir = src_dir.join("read");
	let sub_dir = src_dir.join("subs");

	let mut inotify = Inotify::init().expect("Couldn't start inotify");
	inotify
		.watches()
		.add(&read_dir, WatchMask::CREATE | WatchMask::DELETE)
		.expect("Failed to watch read dir");
	inotify
		.watches()
		.add(
			&sub_dir,
			WatchMask::CREATE | WatchMask::DELETE | WatchMask::MODIFY,
		)
		.expect("Failed to watch subs dir");

	refresh(&read_dir, &sub_dir, &read_articles, &subscriptions, &base64).await;
	let mut counter = 0u8;
	loop {
		counter += 1;
		if counter == 5
			|| inotify
				.read_events(&mut [0])
				.ok()
				.map(|mut i| i.next())
				.is_some()
		{
			refresh(&read_dir, &sub_dir, &read_articles, &subscriptions, &base64).await;
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
}

async fn refresh(
	read_dir: &Path,
	sub_dir: &Path,
	read_articles: &Arc<RwLock<BTreeSet<String>>>,
	subscriptions: &Arc<RwLock<BTreeMap<String, Arc<Channel>>>>,
	base64: &GeneralPurpose,
) {
	{
		let mut read_dir = tokio::fs::read_dir(read_dir)
			.await
			.expect("Couldn't read read_dir");
		let mut read_articles = read_articles.write().await;
		read_articles.clear();
		while let Ok(Some(entry)) = read_dir.next_entry().await {
			let name = entry.file_name();
			let Some(name) = name.to_str() else {
                eprintln!("File's name is not utf8");
                continue;
            };
			let Ok(data) = base64.decode(name) else {
                eprintln!("File {name}'s name is not base64");
                continue;
            };
			let Ok(id) = String::from_utf8(data) else {
                eprintln!("File {name}'s name is not base64'd utf8");
                continue;
            };
			read_articles.insert(id);
		}
	}
	{
		let mut sub_dir = tokio::fs::read_dir(sub_dir)
			.await
			.expect("Couldn't read sub_dir");
		let mut subscriptions = subscriptions.write().await;
		let mut still_in_subs = BTreeSet::default();
		while let Ok(Some(entry)) = sub_dir.next_entry().await {
			// Get the subscription's URL
			let name = entry.file_name();
			let Some(name) = name.to_str() else {
                eprintln!("File's name is not utf8");
                continue;
            };
			let Ok(data) = base64.decode(name) else {
                eprintln!("File {name}'s name is not base64");
                continue;
            };
			let Ok(pub_url) = String::from_utf8(data) else {
                eprintln!("File {name}'s name is not base64'd utf8");
                continue;
            };
			// Get the subscription's contents
			let file = match OpenOptions::new().read(true).open(entry.path()).await {
				Err(e) => {
					eprintln!("Couldn't read {name}, {e}");
					continue;
				}
				Ok(f) => f,
			};
			let file = BufReader::new(file.into_std().await);
			let Ok(channel) = Channel::read_from(file) else {
                eprintln!("RSS in {name} is invalid");
                continue;
            };

			still_in_subs.insert(pub_url.clone());
			let sub = Arc::make_mut(subscriptions.entry(pub_url).or_default());
			sub.merge(&channel);
		}
		subscriptions.retain(|k, _| still_in_subs.contains(k));
	}
}
