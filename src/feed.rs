use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename = "link")]
pub struct Link {
	pub(crate) r#type: String,
	pub(crate) href: String,
}

// Takes an HTML page, and returns all linked RSS feeds.
pub fn find_feed(from: &str) -> Vec<String> {
	let link_ex = "<link[^>]*>";
	let link_ex = regex::Regex::new(link_ex).expect("Bad link regex");
	link_ex
		.find_iter(from)
		.map(|matched| format!("{}</link>", matched.as_str()))
		.flat_map(|matched| serde_xml_rs::from_str::<Link>(&matched))
		.filter(|link| link.r#type == "application/rss+xml")
		.map(|link| link.href)
		.collect()
}

#[cfg(test)]
mod tests {
	use super::find_feed;

	#[test]
	fn finds_link_in_html() {
		let yt_html = include_str!("./test-data/youtube.html");
		let found = find_feed(yt_html);
		assert_eq!(
			found,
			["https://www.youtube.com/feeds/videos.xml?channel_id=UCBR8-60-B28hp2BmDPdntcQ"]
		);
	}
}
