use std::{collections::BTreeMap, sync::Arc, time::Duration};

use http_cache_reqwest::{CACacheManager, Cache, HttpCache, HttpCacheOptions};
use reqwest::{Client, Response};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use tokio::{sync::RwLock, task::JoinHandle};

type RequestOutcome = Result<Response, reqwest_middleware::Error>;

pub struct Fetcher {
	client: Arc<ClientWithMiddleware>,
	in_progress: Arc<RwLock<BTreeMap<String, JoinHandle<RequestOutcome>>>>,
}

impl Fetcher {
	#[must_use]
	pub fn new() -> Self {
		let client = ClientBuilder::new(Client::new())
			.with(Cache(HttpCache {
				mode: http_cache_reqwest::CacheMode::Default,
				manager: CACacheManager::default(),
				options: HttpCacheOptions::default(),
			}))
			.build();
		let client = Arc::new(client);
		let in_progress = Arc::new(RwLock::new(BTreeMap::default()));
		Self {
			client,
			in_progress,
		}
	}

	pub async fn start_download<S: ToString>(&self, url: S) {
		let url = url.to_string();
		let client = self.client.clone();
		if self.in_progress.read().await.contains_key(&url) {
			return;
		}
		self.in_progress.write().await.insert(
			url.clone(),
			tokio::task::spawn(async move {
				client
					.get(url)
					.timeout(Duration::from_secs(30))
					.send()
					.await
			}),
		);
	}

	pub async fn try_finish(&self, url: &str) -> Option<RequestOutcome> {
		let mut in_progress = self.in_progress.write().await;
		let handle = in_progress.remove(url)?;
		if !handle.is_finished() {
			in_progress.insert(url.to_string(), handle);
			return None;
		}
		match handle.await {
			Ok(o) => Some(o),
			Err(e) => {
				eprintln!("{e}");
				None
			}
		}
	}
}

impl Default for Fetcher {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod test {
	use super::Fetcher;

	#[tokio::test]
	async fn try_download_example_dot_com() {
		let fetcher = Fetcher::default();
		let outcome;
		fetcher.start_download("https://example.com").await;

		loop {
			if let Some(new_outcome) = fetcher.try_finish("https://example.com").await {
				outcome = new_outcome;
				break;
			}
		}
		let text = outcome.unwrap().text().await.unwrap();
		eprintln!("{text}");
	}
}
