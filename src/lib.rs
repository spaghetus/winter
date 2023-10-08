#![allow(clippy::module_name_repetitions)]
pub mod feed;
pub mod state;
pub mod fetch;
pub mod document;

lazy_static::lazy_static! {
	static ref FETCHER: fetch::Fetcher = fetch::Fetcher::new();
}
