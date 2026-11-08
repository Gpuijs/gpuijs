//! `<video>` and `<audio>`, played by AVFoundation's `AVPlayer`. A video's frames come from an
//! `AVPlayerItemVideoOutput` as the YCbCr pixel buffers gpui's `surface` draws.

// objc 0.2's macros test a `cargo-clippy` feature this crate doesn't declare.
#![allow(unexpected_cfgs)]

use core_foundation::base::TCFType;
use core_video::pixel_buffer::{
    CVPixelBuffer, CVPixelBufferRef, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
};
use objc::runtime::{BOOL, NO, Object, YES};
use objc::{class, msg_send, sel, sel_impl};
use serde_json::{Value, json};

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMTimeGetSeconds(time: CMTime) -> f64;
    fn CMTimeMakeWithSeconds(seconds: f64, timescale: i32) -> CMTime;
}

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}

/// `AVPlayerItemStatus`.
const READY: isize = 1;
const FAILED: isize = 2;

/// How often a playing element's `timeupdate` goes to JS, as browsers fire it.
const TIME_UPDATES: f64 = 0.25;

pub struct Player {
    /// What it plays: a file path or an http(s) URL.
    pub src: String,
    player: *mut Object,
    item: *mut Object,
    /// Video only.
    output: Option<*mut Object>,
    /// The latest decoded frame.
    pub frame: Option<CVPixelBuffer>,
    pub looping: bool,
    muted: bool,
    ready: bool,
    failed: bool,
    playing: bool,
    reported: f64,
    /// Frames left to look for a picture while paused: the first one, or where a seek landed.
    awaiting_frame: u32,
}

/// A paused video's next picture is decoded within this many display frames.
const FRAME_WAIT: u32 = 30;

/// An autoreleased `NSString`.
unsafe fn ns_string(s: &str) -> *mut Object {
    unsafe {
        let string: *mut Object = msg_send![class!(NSString), alloc];
        let string: *mut Object = msg_send![string, initWithBytes: s.as_ptr() length: s.len() encoding: 4usize];
        msg_send![string, autorelease]
    }
}

impl Player {
    pub fn open(src: &str, video: bool) -> Option<Player> {
        unsafe {
            let remote = src.starts_with("http://") || src.starts_with("https://");
            let url: *mut Object = match remote {
                true => msg_send![class!(NSURL), URLWithString: ns_string(src)],
                false => msg_send![class!(NSURL), fileURLWithPath: ns_string(src)],
            };
            if url.is_null() {
                return None;
            }
            let item: *mut Object = msg_send![class!(AVPlayerItem), playerItemWithURL: url];
            let _: *mut Object = msg_send![item, retain];
            let output = video.then(|| {
                // The format gpui's surface pipeline draws, in buffers Metal can read.
                let keys: [*mut Object; 2] = [
                    kCVPixelBufferPixelFormatTypeKey as *mut Object,
                    kCVPixelBufferMetalCompatibilityKey as *mut Object,
                ];
                let format: *mut Object = msg_send![class!(NSNumber), numberWithUnsignedInt: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange];
                let metal: *mut Object = msg_send![class!(NSNumber), numberWithBool: YES];
                let values = [format, metal];
                let attrs: *mut Object = msg_send![class!(NSDictionary), dictionaryWithObjects: values.as_ptr() forKeys: keys.as_ptr() count: 2usize];
                let output: *mut Object = msg_send![class!(AVPlayerItemVideoOutput), alloc];
                let output: *mut Object = msg_send![output, initWithPixelBufferAttributes: attrs];
                let _: () = msg_send![item, addOutput: output];
                output
            });
            let player: *mut Object = msg_send![class!(AVPlayer), alloc];
            let player: *mut Object = msg_send![player, initWithPlayerItem: item];
            Some(Player {
                src: src.to_owned(),
                player,
                item,
                output,
                frame: None,
                looping: false,
                muted: false,
                ready: false,
                failed: false,
                playing: false,
                reported: 0.0,
                awaiting_frame: if video { FRAME_WAIT } else { 0 },
            })
        }
    }

    pub fn play(&mut self) {
        unsafe {
            // Played to the end, it starts over, as a media element does.
            if self.ended() {
                self.seek(0.0);
            }
            let _: () = msg_send![self.player, play];
        }
        self.playing = true;
    }

    pub fn pause(&mut self) {
        unsafe {
            let _: () = msg_send![self.player, pause];
        }
        self.playing = false;
    }

    pub fn seek(&mut self, seconds: f64) {
        unsafe {
            let at = CMTimeMakeWithSeconds(seconds.max(0.0), 600);
            let exact = CMTimeMakeWithSeconds(0.0, 600);
            let _: () = msg_send![self.player, seekToTime: at toleranceBefore: exact toleranceAfter: exact];
        }
        self.reported = seconds;
        if self.output.is_some() {
            self.awaiting_frame = FRAME_WAIT;
        }
    }

    pub fn set_volume(&mut self, volume: f64) {
        unsafe {
            let _: () = msg_send![self.player, setVolume: volume.clamp(0.0, 1.0) as f32];
        }
    }

    pub fn set_rate(&mut self, rate: f64) {
        unsafe {
            let _: () = msg_send![self.player, setDefaultRate: rate as f32];
            if self.playing {
                let _: () = msg_send![self.player, setRate: rate as f32];
            }
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        if muted != self.muted {
            self.muted = muted;
            let flag: BOOL = if muted { YES } else { NO };
            unsafe {
                let _: () = msg_send![self.player, setMuted: flag];
            }
        }
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn current_time(&self) -> f64 {
        unsafe { CMTimeGetSeconds(msg_send![self.player, currentTime]) }
    }

    /// NaN until known, as `HTMLMediaElement.duration`; infinite for a live stream.
    pub fn duration(&self) -> f64 {
        if !self.ready {
            return f64::NAN;
        }
        unsafe { CMTimeGetSeconds(msg_send![self.item, duration]) }
    }

    /// The video's size in CSS px, once known.
    pub fn natural_size(&self) -> Option<(f32, f32)> {
        let size: CGSize = unsafe { msg_send![self.item, presentationSize] };
        (size.width > 0.0).then_some((size.width as f32, size.height as f32))
    }

    fn ended(&self) -> bool {
        let duration = self.duration();
        duration.is_finite() && self.current_time() >= duration - 0.05
    }

    /// Takes in what changed since the last call: the media events JS should hear, as
    /// (`event`, fields). Returns whether it needs another frame soon.
    pub fn poll(&mut self, events: &mut Vec<Value>) -> bool {
        if self.failed {
            return false;
        }
        if !self.ready {
            let status: isize = unsafe { msg_send![self.item, status] };
            match status {
                READY => {
                    self.ready = true;
                    let (w, h) = self.natural_size().unwrap_or_default();
                    let duration = self.duration();
                    events.push(json!({
                        "event": "loadedmetadata",
                        // JSON has no NaN or infinity: a live stream's is `null`.
                        "duration": duration.is_finite().then_some(duration),
                        "videoWidth": w,
                        "videoHeight": h,
                    }));
                }
                FAILED => {
                    self.failed = true;
                    events.push(json!({ "event": "error" }));
                    return false;
                }
                _ => return true,
            }
        }
        self.copy_frame();
        if !self.playing {
            self.awaiting_frame = self.awaiting_frame.saturating_sub(1);
            return self.awaiting_frame > 0;
        }
        let now = self.current_time();
        if self.ended() {
            if self.looping {
                self.seek(0.0);
                unsafe {
                    let _: () = msg_send![self.player, play];
                }
            } else {
                self.playing = false;
                events.push(json!({ "event": "ended", "currentTime": now }));
                return false;
            }
        } else if (now - self.reported).abs() >= TIME_UPDATES {
            self.reported = now;
            events.push(json!({ "event": "timeupdate", "currentTime": now }));
        }
        true
    }

    fn copy_frame(&mut self) {
        let Some(output) = self.output else { return };
        unsafe {
            let at: CMTime = msg_send![output, itemTimeForHostTime: CACurrentMediaTime()];
            let new: BOOL = msg_send![output, hasNewPixelBufferForItemTime: at];
            if new == NO {
                return;
            }
            let shown: *mut CMTime = std::ptr::null_mut();
            let buffer: CVPixelBufferRef = msg_send![output, copyPixelBufferForItemTime: at itemTimeForDisplay: shown];
            if !buffer.is_null() {
                self.frame = Some(CVPixelBuffer::wrap_under_create_rule(buffer));
                self.awaiting_frame = 0;
            }
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.player, pause];
            let _: () = msg_send![self.player, release];
            let _: () = msg_send![self.item, release];
            if let Some(output) = self.output {
                let _: () = msg_send![output, release];
            }
        }
    }
}

/// `m:ss`, as a media control shows a time.
pub fn clock(seconds: f64) -> String {
    let s = if seconds.is_finite() { seconds.max(0.0) as u64 } else { 0 };
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half a second of 8 kHz mono silence.
    fn wav() -> std::path::PathBuf {
        let samples = 4000u32;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((36 + samples * 2).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend([16u32, 0x0001_0001, 8000, 16000].iter().flat_map(|v| v.to_le_bytes()));
        bytes.extend([2u16, 16].iter().flat_map(|v| v.to_le_bytes()));
        bytes.extend(b"data");
        bytes.extend((samples * 2).to_le_bytes());
        bytes.resize(bytes.len() + samples as usize * 2, 0);
        let path = std::env::temp_dir().join(format!("gpuijs-media-{}.wav", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    // AVFoundation reports the item ready on the main queue, which only the app's run loop
    // serves: this sends every message the player uses, so a misspelt one fails here, not in an app.
    #[test]
    fn drives_avplayer() {
        let path = wav();
        let mut player = Player::open(path.to_str().unwrap(), true).unwrap();
        let mut events = Vec::new();
        assert!(player.poll(&mut events), "keeps polling while it loads");
        player.set_muted(true);
        player.set_volume(0.5);
        player.set_rate(2.0);
        player.play();
        assert!(player.playing());
        player.seek(0.25);
        player.pause();
        assert!(!player.playing());
        player.ready = true;
        player.copy_frame();
        let _ = (player.duration(), player.current_time(), player.natural_size(), player.ended());
        drop(player);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn clock_reads_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(75.9), "1:15");
        assert_eq!(clock(f64::NAN), "0:00");
    }
}
