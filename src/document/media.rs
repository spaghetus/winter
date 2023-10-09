#[cfg(feature = "gui")]
use eframe::egui::Ui;
use std::{
	fmt::Display,
	path::PathBuf,
	sync::atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

use crate::FETCHER;

lazy_static::lazy_static! {
	static ref TMP: tempdir::TempDir = tempdir::TempDir::new("media_cache").expect("Couldn't make temporary dir");
	static ref COUNTER: AtomicU64 = AtomicU64::new(0);
}

pub enum MaybeLoaded<Inner: TryFrom<Vec<u8>>> {
	NotStarted(String),
	Working(String),
	Done(String, Result<Inner, Inner::Error>),
	Failed(String, reqwest_middleware::Error),
	BadStatus(u16),
}

impl<I: TryFrom<Vec<u8>>> MaybeLoaded<I> {
	pub async fn tick(&mut self) {
		if let MaybeLoaded::NotStarted(url) = &self {
			let url = url.to_string();
			FETCHER.start_download(&url).await;
			*self = MaybeLoaded::Working(url);
			return;
		}
		let MaybeLoaded::Working(url) = self else {
					return;
				};
		let url = (*url).to_string();
		let Some(completion) = FETCHER.try_finish(&url).await else {return;};
		let response = match completion {
			Ok(r) => r,
			Err(e) => {
				*self = MaybeLoaded::Failed(url.clone(), e);
				return;
			}
		};
		let status = response.status();
		if !status.is_success() {
			*self = MaybeLoaded::BadStatus(status.as_u16());
			return;
		}
		let body: Vec<u8> = response
			.bytes()
			.await
			.expect("Response body is empty?")
			.into_iter()
			.collect();
		*self = MaybeLoaded::Done(url, TryInto::try_into(body));
	}
}

pub struct Video {
	cache_path: PathBuf,
}

#[derive(Error, Debug)]
pub enum VideoError {
	NoGUI,
	IoError(#[from] std::io::Error),
}
impl Display for VideoError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{self:?}")
	}
}

impl TryFrom<Vec<u8>> for Video {
	type Error = VideoError;

	#[cfg(feature = "gui")]
	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		let path = TMP
			.path()
			.to_path_buf()
			.join(COUNTER.fetch_add(1, Ordering::Relaxed).to_string());
		std::fs::write(&path, value)?;
		Ok(Video { cache_path: path })
	}
	#[cfg(not(feature = "gui"))]
	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		Err(VideoError::NoGUI)
	}
}

#[cfg(feature = "gui")]
impl Video {
	pub fn show(&mut self, ui: &mut Ui) {
		if ui.button("Play Video").clicked() {
			open::that(&self.cache_path).expect("Failed to open video in system app");
		}
	}
}

pub struct Audio {
	cache_path: PathBuf,
}

#[derive(Error, Debug)]
pub enum AudioError {
	NoGUI,
	IoError(#[from] std::io::Error),
}
impl Display for AudioError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{self:?}")
	}
}

impl TryFrom<Vec<u8>> for Audio {
	type Error = AudioError;

	#[cfg(feature = "gui")]
	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		let path = TMP
			.path()
			.to_path_buf()
			.join(COUNTER.fetch_add(1, Ordering::Relaxed).to_string());
		std::fs::write(&path, value)?;
		Ok(Audio { cache_path: path })
	}
	#[cfg(not(feature = "gui"))]
	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		Err(AudioError::NoGUI)
	}
}

#[cfg(feature = "gui")]
impl Audio {
	pub fn show(&mut self, ui: &mut Ui) {
		if ui.button("Play Audio").clicked() {
			open::that(&self.cache_path).expect("Failed to open audio in system app");
		}
	}
}

// pub struct Audio {
// 	data: Vec<u8>,
// 	#[cfg(feature = "gui")]
// 	manager: AudioManager<DefaultBackend>,
// 	#[cfg(feature = "gui")]
// 	handle: StaticSoundHandle,
// 	length: Duration,
// 	playing: bool,
// 	paused: bool,
// }

// #[derive(Error, Debug)]
// pub enum AudioError {
// 	NoGUI,
// 	#[cfg(feature = "gui")]
// 	FromFileError(#[from] FromFileError),
// 	#[cfg(feature = "gui")]
// 	BackendError(#[from] kira::manager::backend::cpal::Error),
// 	#[cfg(feature = "gui")]
// 	PlaySoundError(#[from] PlaySoundError<()>),
// 	#[cfg(feature = "gui")]
// 	CommandError(#[from] CommandError),
// }
// impl Display for AudioError {
// 	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
// 		write!(f, "{self:?}")
// 	}
// }

// impl TryFrom<Vec<u8>> for Audio {
// 	type Error = AudioError;

// 	#[cfg(feature = "gui")]
// 	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
// 		let mut manager: AudioManager<DefaultBackend> =
// 			AudioManager::new(AudioManagerSettings::default())?;
// 		let data =
// 			StaticSoundData::from_cursor(Cursor::new(value.clone()), StaticSoundSettings::default())?;
// 		let length = data.duration();
// 		let handle = manager.play(data)?;
// 		manager.pause(Tween {
// 			start_time: kira::StartTime::Immediate,
// 			duration: Duration::ZERO,
// 			easing: kira::tween::Easing::Linear,
// 		})?;
// 		Ok(Self {
// 			data: value,
// 			manager,
// 			handle,
// 			length,
// 			playing: false,
// 			paused: true,
// 		})
// 	}

// 	#[cfg(not(feature = "gui"))]
// 	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
// 		Err(AudioError::NoGUI)
// 	}
// }

// const ZERO_TWEEN: Tween = Tween {
// 	start_time: kira::StartTime::Immediate,
// 	duration: Duration::ZERO,
// 	easing: kira::tween::Easing::Linear,
// };

// #[cfg(feature = "gui")]
// impl Audio {
// 	pub fn play(&mut self) {
// 		if !self.playing {
// 			let data = StaticSoundData::from_cursor(
// 				Cursor::new(self.data.clone()),
// 				StaticSoundSettings::default(),
// 			)
// 			.unwrap();
// 			self.handle = self.manager.play(data).unwrap();
// 			self.handle.pause(ZERO_TWEEN).unwrap();
// 			self.playing = true;
// 			self.paused = false;
// 			return;
// 		}
// 		if self.paused {
// 			self.manager.resume(ZERO_TWEEN).unwrap();
// 		}
// 	}

// 	pub fn pause(&mut self) {
// 		if self.playing && !self.paused {
// 			self.handle.pause(ZERO_TWEEN).unwrap();
// 		}
// 	}

// 	pub fn show(&mut self, ui: &mut Ui) {
// 		let tween = Tween {
// 			start_time: kira::StartTime::Immediate,
// 			duration: Duration::from_secs_f32(0.2),
// 			easing: kira::tween::Easing::Linear,
// 		};
// 		let playing = self.playing && !self.paused;
// 		let symbol = if playing {"||"} else {">"};
// 		ui.horizontal(|ui| {
// 			let mut position = self.handle.position();
// 			let finished = playing && position >= self.length.as_secs_f64() - 0.5;
// 			if finished {
// 				self.handle.stop(ZERO_TWEEN).unwrap();
// 				self.playing = false;
// 			}
// 			if ui.button(symbol).clicked() {
// 				if playing {
// 					self.pause();
// 				} else {
// 					self.play();
// 				}
// 			}
// 			if playing {
// 				ui.ctx().request_repaint();
// 			}
// 			#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
// 			let minutes = position.div_euclid(60.0) as u32;
// 			#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
// 			let seconds = position.rem_euclid(60.0) as u32;
// 			let time = format!("{minutes:02}:{seconds:02}",);
// 			ui.label(time);
// 			ui.separator();
// 			ui.allocate_ui(Vec2::new(ui.available_width(), 0.0), |ui| {
// 				let slider = ui.add(
// 					eframe::egui::widgets::Slider::new(
// 						&mut position,
// 						0.0..=(self.length.as_secs_f64() - 0.5),
// 					)
// 					.show_value(false).trailing_fill(true),
// 				);
// 				if slider.drag_released() || slider.clicked() {
// 					println!("SEEK TO {position}");
// 					self.handle.seek_to(position).expect("Failed to seek");
// 					self.handle.pause(ZERO_TWEEN).expect("Failed to pause after seek");
// 				}
// 			});
// 		});
// 	}
// }
