use std::{
	collections::{BTreeMap, BTreeSet},
	io::BufReader,
	path::{Path, PathBuf},
	str::FromStr,
	sync::Arc,
	time::Duration,
};

use base64::{
	engine::{GeneralPurpose, GeneralPurposeConfig},
	Engine,
};
use inotify::{Inotify, WatchMask};
use syndication::Feed;
use tokio::{fs::OpenOptions, sync::RwLock};

use super::Merge;

pub async fn inotify_loop(
	src_dir: PathBuf,
	read_articles: Arc<RwLock<BTreeSet<String>>>,
	subscriptions: Arc<RwLock<BTreeMap<String, Arc<Feed>>>>,
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
			counter = 0;
			refresh(&read_dir, &sub_dir, &read_articles, &subscriptions, &base64).await;
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
}

async fn refresh(
	read_dir: &Path,
	sub_dir: &Path,
	read_articles: &Arc<RwLock<BTreeSet<String>>>,
	subscriptions: &Arc<RwLock<BTreeMap<String, Arc<Feed>>>>,
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
			// let file = match OpenOptions::new().read(true).open(entry.path()).await {
			// 	Err(e) => {
			// 		eprintln!("Couldn't read {name}, {e}");
			// 		continue;
			// 	}
			// 	Ok(f) => f,
			// };
			let file = match tokio::fs::read_to_string(entry.path()).await {
				Err(e) => {
					eprintln!("Couldn't read {name}, {e}");
					continue;
				}
				Ok(f) => f,
			};
			let channel = match Feed::from_str(&file) {
				Ok(c) => c,
				Err(e) => {
					eprintln!("RSS in {name} is invalid: {e}");
					continue;
				}
            };

			still_in_subs.insert(pub_url.clone());
			let sub = Arc::make_mut(subscriptions.entry(pub_url).or_insert_with(
				|| match channel {
					Feed::RSS(_) => Arc::new(Feed::RSS(Default::default())),
					Feed::Atom(_) => Arc::new(Feed::Atom(Default::default())),
				},
			));
			sub.merge(&channel);
		}
		subscriptions.retain(|k, _| still_in_subs.contains(k));
	}
}
