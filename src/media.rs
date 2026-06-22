use crate::identity::Identity;
use crate::layout::LayoutBox;
#[cfg(feature = "media-ffmpeg")]
use ffmpeg_next as ffmpeg;
use std::collections::HashMap;
#[cfg(feature = "media-ffmpeg")]
use std::path::PathBuf;
#[cfg(feature = "media-ffmpeg")]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[cfg(feature = "media-ffmpeg")]
static TEMP_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct MediaCache {
    videos: HashMap<String, DecodedVideo>,
    aliases: HashMap<String, String>,
    started_at: Instant,
}

struct DecodedVideo {
    frames: Vec<VideoFrame>,
    duration_secs: f64,
    current_frame: usize,
}

struct VideoFrame {
    at_secs: f64,
    #[allow(dead_code)]
    image: peniko::ImageData,
}

impl Default for MediaCache {
    fn default() -> Self {
        Self {
            videos: HashMap::new(),
            aliases: HashMap::new(),
            started_at: Instant::now(),
        }
    }
}

impl MediaCache {
    pub(crate) fn load(root: &LayoutBox, base_url: Option<&str>, identity: &Identity) -> Self {
        let mut cache = Self::default();
        cache.load_missing(root, base_url, identity);
        cache
    }

    pub(crate) fn load_missing(
        &mut self,
        root: &LayoutBox,
        base_url: Option<&str>,
        identity: &Identity,
    ) {
        let mut sources = Vec::new();
        collect_media_srcs(root, base_url, &mut sources);

        if sources.is_empty() {
            return;
        }

        if !init_decoder() {
            eprintln!("Aurora: media playback requires the media-ffmpeg feature");
            return;
        }

        for (raw, resolved) in sources {
            if raw != resolved {
                self.aliases.insert(raw.clone(), resolved.clone());
            }
            if self.videos.contains_key(&resolved) {
                continue;
            }
            match decode_video(&resolved, identity) {
                Ok(video) => {
                    self.videos.insert(resolved, video);
                }
                Err(error) => eprintln!("Aurora: failed to decode media {raw}: {error}"),
            }
        }
    }

    pub(crate) fn update(&mut self) -> bool {
        let elapsed = self.started_at.elapsed().as_secs_f64();
        let mut changed = false;
        for video in self.videos.values_mut() {
            changed |= video.update(elapsed);
        }
        changed
    }

    #[allow(dead_code)]
    pub(crate) fn frame(&self, src: &str) -> Option<&peniko::ImageData> {
        let src = self.aliases.get(src).map(String::as_str).unwrap_or(src);
        self.videos
            .get(src)
            .and_then(|video| video.frames.get(video.current_frame))
            .map(|frame| &frame.image)
    }

    pub(crate) fn has_active_media(&self) -> bool {
        self.videos.values().any(|video| video.frames.len() > 1)
    }
}

impl DecodedVideo {
    fn update(&mut self, elapsed_secs: f64) -> bool {
        if self.frames.len() <= 1 {
            return false;
        }

        let playback_time = if self.duration_secs > 0.0 {
            elapsed_secs % self.duration_secs
        } else {
            elapsed_secs
        };

        let frame_index = self
            .frames
            .partition_point(|frame| frame.at_secs <= playback_time)
            .saturating_sub(1)
            .min(self.frames.len() - 1);

        if frame_index != self.current_frame {
            self.current_frame = frame_index;
            return true;
        }
        false
    }
}

fn collect_media_srcs(node: &LayoutBox, base_url: Option<&str>, out: &mut Vec<(String, String)>) {
    if let Some(src) = node.media_src() {
        let resolved = if let Some(base) = base_url {
            crate::fetch::resolve_relative_url(base, src).unwrap_or_else(|_| src.to_string())
        } else {
            src.to_string()
        };
        if !out.iter().any(|(_, existing)| existing == &resolved) {
            out.push((src.to_string(), resolved));
        }
    }

    for child in node.children() {
        collect_media_srcs(child, base_url, out);
    }
}

fn decode_video(url: &str, identity: &Identity) -> Result<DecodedVideo, String> {
    decode_video_impl(url, identity)
}

#[cfg(not(feature = "media-ffmpeg"))]
fn init_decoder() -> bool {
    false
}

#[cfg(feature = "media-ffmpeg")]
fn init_decoder() -> bool {
    match ffmpeg::init() {
        Ok(()) => true,
        Err(error) => {
            eprintln!("Aurora: failed to initialize ffmpeg: {error}");
            false
        }
    }
}

#[cfg(not(feature = "media-ffmpeg"))]
fn decode_video_impl(_url: &str, _identity: &Identity) -> Result<DecodedVideo, String> {
    Err("built without the media-ffmpeg feature".to_string())
}

#[cfg(feature = "media-ffmpeg")]
fn decode_video_impl(url: &str, identity: &Identity) -> Result<DecodedVideo, String> {
    let bytes = crate::fetch::fetch_bytes(url, identity).map_err(|error| error.to_string())?;
    let path = write_temp_media_file(&bytes).map_err(|error| error.to_string())?;
    let result = decode_video_file(&path);
    let _ = std::fs::remove_file(&path);
    result
}

#[cfg(feature = "media-ffmpeg")]
fn write_temp_media_file(bytes: &[u8]) -> std::io::Result<PathBuf> {
    let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("aurora-media-{}-{id}.bin", std::process::id()));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(feature = "media-ffmpeg")]
fn decode_video_file(path: &PathBuf) -> Result<DecodedVideo, String> {
    let mut input = ffmpeg::format::input(path).map_err(|error| error.to_string())?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| "no video stream found".to_string())?;
    let stream_index = stream.index();
    let time_base = stream.time_base();

    let codec_context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|error| error.to_string())?;
    let mut decoder = codec_context
        .decoder()
        .video()
        .map_err(|error| error.to_string())?;
    let mut scaler = ffmpeg::software::scaling::context::Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        ffmpeg::format::Pixel::RGBA,
        decoder.width(),
        decoder.height(),
        ffmpeg::software::scaling::flag::Flags::BILINEAR,
    )
    .map_err(|error| error.to_string())?;

    let mut frames = Vec::new();
    for (packet_stream, packet) in input.packets() {
        if packet_stream.index() != stream_index {
            continue;
        }
        decoder
            .send_packet(&packet)
            .map_err(|error| error.to_string())?;
        receive_frames(&mut decoder, &mut scaler, time_base, &mut frames)?;
    }

    decoder.send_eof().map_err(|error| error.to_string())?;
    receive_frames(&mut decoder, &mut scaler, time_base, &mut frames)?;

    if frames.is_empty() {
        return Err("no decodable video frames found".to_string());
    }

    let duration_secs = frames
        .last()
        .map(|frame| frame.at_secs)
        .unwrap_or(0.0)
        .max(0.0);

    Ok(DecodedVideo {
        frames,
        duration_secs,
        current_frame: 0,
    })
}

#[cfg(feature = "media-ffmpeg")]
fn receive_frames(
    decoder: &mut ffmpeg::decoder::Video,
    scaler: &mut ffmpeg::software::scaling::context::Context,
    time_base: ffmpeg::Rational,
    frames: &mut Vec<VideoFrame>,
) -> Result<(), String> {
    let mut decoded = ffmpeg::util::frame::Video::empty();
    while decoder.receive_frame(&mut decoded).is_ok() {
        let mut rgba = ffmpeg::util::frame::Video::empty();
        scaler
            .run(&decoded, &mut rgba)
            .map_err(|error| error.to_string())?;
        frames.push(VideoFrame {
            at_secs: frame_time_secs(&decoded, time_base, frames.len()),
            image: image_data_from_frame(&rgba),
        });
    }
    Ok(())
}

#[cfg(feature = "media-ffmpeg")]
fn frame_time_secs(
    frame: &ffmpeg::util::frame::Video,
    time_base: ffmpeg::Rational,
    index: usize,
) -> f64 {
    frame
        .pts()
        .map(|pts| pts as f64 * time_base.numerator() as f64 / time_base.denominator() as f64)
        .unwrap_or(index as f64 / 30.0)
}

#[cfg(feature = "media-ffmpeg")]
fn image_data_from_frame(frame: &ffmpeg::util::frame::Video) -> peniko::ImageData {
    let width = frame.width();
    let height = frame.height();
    let stride = frame.stride(0);
    let row_len = width as usize * 4;
    let mut pixels = Vec::with_capacity(row_len * height as usize);
    let data = frame.data(0);

    for row in 0..height as usize {
        let start = row * stride;
        let end = start + row_len;
        pixels.extend_from_slice(&data[start..end]);
    }

    peniko::ImageData {
        data: peniko::Blob::from(pixels),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width,
        height,
    }
}
