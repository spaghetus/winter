/// Copied from the `syndication` crate
/// I don't want to figure out cargo vendoring rn so I'm doing this instead

use std::str::FromStr;

#[derive(Clone)]
pub enum Feed {
    Atom(atom_syndication::Feed),
    RSS(rss::Channel),
}

impl FromStr for Feed {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match atom_syndication::Feed::from_str(s) {
            Ok(feed) => Ok(Feed::Atom(feed)),
            _ => match rss::Channel::from_str(s) {
                Ok(feed) => Ok(Feed::RSS(feed)),
                _ => Err("Could not parse XML as Atom or RSS from input"),
            },
        }
    }
}

impl ToString for Feed {
    fn to_string(&self) -> String {
        match self {
            Feed::Atom(atom_feed) => atom_feed.to_string(),
            Feed::RSS(rss_channel) => rss_channel.to_string(),
        }
    }
}