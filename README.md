# WINTER

(*Willow's Interactive Network Text and Entertainment Reader*)

A simple RSS reader made with Rust and EGUI.

## Features

* [x] No Electron
  * Automatically loading foreign HTML into an ancient version of Chrome with access to system APIs from JavaScript seems dangerous, doesn't it?
* [x] Interpret a meaningful subset of HTML4
* [x] Sync across multiple devices without a special server
  * Just sync the database however you normally sync files, and it should work even if you're running multiple instances of WINTER at the same time.
* [x] Support for playing audio and video
  * Currently this just caches them locally and plays them with your system media player, but I would like to embed a player at some point.
* [ ] Integration with yt-dlp for embedding YouTube links
  * The way YouTube represents videos is very annoying
