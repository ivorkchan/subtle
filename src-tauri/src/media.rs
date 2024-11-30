extern crate ffmpeg_next as ffmpeg;

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::ipc::{self, Channel, InvokeResponseBody, Response};
use tauri::State;

pub mod internal;
pub(crate) use internal::MediaPlayback;

pub struct PlaybackRegistry {
    next_id: i32,
    table: HashMap<i32, MediaPlayback>
}

impl PlaybackRegistry {
    pub fn new() -> PlaybackRegistry {
        PlaybackRegistry {
            next_id: 0,
            table: HashMap::new()
        } 
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum MediaEvent<'a> {
    #[serde(rename_all = "camelCase")]
    Done,
    #[serde(rename_all = "camelCase")]
    IntensityList {
        start: i64,
        end: i64,
        data: Vec<f32>,
    },
    #[serde(rename_all = "camelCase")]
    MediaStatus {
        audio_index: i32,
        video_index: i32,
        duration: f64,
        streams: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    AudioStatus {
        length: i64,
        sample_rate: u32,
    },
    #[serde(rename_all = "camelCase")]
    VideoStatus {
        length: i64,
        framerate: f64,
        out_width: u32,
        out_height: u32,
        width: u32,
        height: u32
    },
    #[serde(rename_all = "camelCase")]
    Debug { message: &'a str },
    #[serde(rename_all = "camelCase")]
    RuntimeError { what: &'a str },
    #[serde(rename_all = "camelCase")]
    Opened {
        id: i32
    },
    #[serde(rename_all = "camelCase")]
    Position {
        value: i64
    },
    #[serde(rename_all = "camelCase")]
    NoStream {},
    #[serde(rename_all = "camelCase")]
    InvalidId {},
}

fn send(channel: &Channel<MediaEvent>, what: MediaEvent) {
    channel.send(what).expect("Error sending event");
}

macro_rules! send_error {
    ($channel:expr, $what:expr) => {
        $channel
            .send(MediaEvent::RuntimeError {
                what: format!("{} (at line {})", AsRef::<str>::as_ref(&$what), line!()).as_str(),
            })
            .expect("Error sending event")
    };
}

fn send_invalid_id(channel: &Channel<MediaEvent>) {
    channel
        .send(MediaEvent::InvalidId {})
        .expect("Error sending event");
}

fn send_done(channel: &Channel<MediaEvent>) {
    channel.send(MediaEvent::Done).expect("Error sending event");
}

#[tauri::command]
pub fn media_status(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>, 
    channel: Channel<MediaEvent>
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel),
    };
    let audio_index: i32 = match playback.audio() {
        Some(c) => c.stream_index().try_into().unwrap(),
        None => -1,
    };
    let video_index: i32 = -1;
    send(
        &channel,
        MediaEvent::MediaStatus {
            audio_index,
            video_index,
            duration: playback.duration(),
            streams: playback.describe_streams(),
        },
    );
}

#[tauri::command]
pub fn audio_status(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>, 
    channel: Channel<MediaEvent>
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel),
    };
    let ctx = match playback.audio() {
        Some(c) => c,
        None => return send(&channel, MediaEvent::NoStream { }),
    };
    send(
        &channel,
        MediaEvent::AudioStatus {
            length: ctx.length(),
            sample_rate: ctx.decoder().rate(),
        },
    );
}

#[tauri::command]
pub fn video_status(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>, 
    channel: Channel<MediaEvent>
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel),
    };
    let ctx = match playback.video() {
        Some(c) => c,
        None => return send(&channel, MediaEvent::NoStream { }),
    };
    let (out_width, out_height) = ctx.output_size();
    let (width, height) = ctx.original_size();
    send(
        &channel,
        MediaEvent::VideoStatus {
            length: ctx.length(), 
            framerate: ctx.framerate().into(),
            width, height,
            out_width, out_height
        }
    );
}

#[tauri::command]
pub fn video_set_size(
    id: i32,
    width: u32, height: u32,
    state: State<Mutex<PlaybackRegistry>>, 
    channel: Channel<MediaEvent>
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel),
    };
    let ctx = match playback.video_mut() {
        Some(c) => c,
        None => return send(&channel, MediaEvent::NoStream { }),
    };
    match ctx.set_output_size((width, height)) {
        Ok(_) => send_done(&channel),
        Err(e) => send_error!(&channel, e)
    }
}

#[tauri::command]
pub fn close_media(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>, 
    channel: Channel<MediaEvent>
) {
    let mut ap = state.lock().unwrap();
    if ap.table.remove(&id).is_none() {
        return send_invalid_id(&channel);
    }
    send_done(&channel);
}

#[tauri::command]
pub fn open_media(
    state: State<Mutex<PlaybackRegistry>>,
    path: &str,
    channel: Channel<MediaEvent>,
) {
    let mut ap = state.lock().unwrap();
    send(&channel, MediaEvent::Debug { message: path });

    let playback = match MediaPlayback::from_file(path) {
        Ok(x) => x,
        Err(e) => return send_error!(&channel, e.to_string()),
    };

    let id = ap.next_id;
    ap.next_id += 1;
    ap.table.insert(id, playback);
    send(&channel, MediaEvent::Opened { id });
}

#[tauri::command]
pub fn open_video(
    id: i32,
    video_id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel),
    };

    let index = 
        if video_id < 0 { None } else { Some(video_id as usize) };
    let video = match playback.open_video(index) {
        Ok(_) => playback.video().unwrap(),
        Err(e) => return send_error!(&channel, e.to_string()),
    };

    send(&channel, MediaEvent::Debug 
        {
            message: format!(
                "opening video {}; len={}:format={}",
                video.stream_index(),
                video.length(),
                video.decoder().format().descriptor().unwrap().name()
            ).as_str(),
        });

    send_done(&channel);
}

#[tauri::command]
pub fn open_audio(
    id: i32,
    audio_id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel),
    };

    let index = 
        if audio_id < 0 { None } else { Some(audio_id as usize) };
    let audio = match playback.open_audio(index) {
        Ok(_) => playback.audio().unwrap(),
        Err(e) => return send_error!(&channel, e.to_string()),
    };

    send(&channel, MediaEvent::Debug 
    {
        message: format!(
            "opening audio {}; len={}:decoder_tb={}:sample_rate={}:sample_fmt={}:channel_layout=0x{:x}",
            audio.stream_index(),
            audio.length(),
            audio.decoder().time_base(),
            audio.decoder().rate(),
            audio.decoder().format().name(),
            audio.decoder().channel_layout().bits()
        ).as_str(),
    });

    send_done(&channel);
}

#[tauri::command]
pub fn seek_audio(
    id: i32,
    position: i64,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel)
    };
    if playback.audio().is_none() {
        return send(&channel, MediaEvent::NoStream { })
    };
    if let Err(e) = playback.seek_audio(position) {
        return send_error!(&channel, e.to_string());
    };

    send_done(&channel);
}

#[tauri::command]
pub fn move_to_next_video_frame(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    {
        let mut ap = state.lock().unwrap();
        let playback = match ap.table.get_mut(&id) {
            Some(x) => x,
            None => return send_invalid_id(&channel)
        };
        if let Err(e) = playback.advance_to_next_video_frame() {
            return send_error!(&channel, e.to_string());
        }
    };
    get_current_video_position(id, state, channel);
}

#[tauri::command]
pub fn move_to_next_audio_frame(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    {
        let mut ap = state.lock().unwrap();
        let playback = match ap.table.get_mut(&id) {
            Some(x) => x,
            None => return send_invalid_id(&channel)
        };
        if let Err(e) = playback.advance_to_next_audio_frame() {
            return send_error!(&channel, format!("advance_to_next_audio_frame: {e}"))
        }
    };
    get_current_audio_position(id, state, channel);
}

#[tauri::command]
pub fn poll_next_audio_frame(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    {
        let mut ap = state.lock().unwrap();
        let playback = match ap.table.get_mut(&id) {
            Some(x) => x,
            None => return send_invalid_id(&channel)
        };
        match playback.poll_next_audio_frame() {
            Ok(true) => (),
            Ok(false) => 
                return send(&channel, MediaEvent::Position { value: -1 }),
            Err(e) => 
                return send_error!(&channel, format!("poll_next_audio_frame: {e}"))
        }
    };
    get_current_audio_position(id, state, channel);
}

#[tauri::command]
pub fn get_current_video_position(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    {
        let mut ap = state.lock().unwrap();
        let playback = match ap.table.get_mut(&id) {
            Some(x) => x,
            None => return send_invalid_id(&channel)
        };
        let video = match playback.video() {
            Some(c) => c,
            None => return send(&channel, MediaEvent::NoStream { })
        };
        if let Some(x) = video.current() {
            return send(&channel, MediaEvent::Position { value: x.position });
        }
    };
    return move_to_next_video_frame(id, state, channel);
}

#[tauri::command]
pub fn get_current_audio_position(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    {
        let mut ap = state.lock().unwrap();
        let playback = match ap.table.get_mut(&id) {
            Some(x) => x,
            None => return send_invalid_id(&channel)
        };
        let audio = match playback.audio() {
            Some(c) => c,
            None => return send(&channel, MediaEvent::NoStream { })
        };
        if let Some(x) = audio.current() {
            return send(&channel, MediaEvent::Position { value: x.position });
        }
    };
    return move_to_next_audio_frame(id, state, channel);
}

/** 
 * returns: [
 *  position    : i64
 *  time        : f64
 *  stride      : u64
 *  length      : u64
 *  rgba_data   : [u8]
 * ]
 * */ 
#[tauri::command]
pub fn send_current_video_frame(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) -> Result<ipc::Response, ()> {
    fn to_byte_slice<'a>(data: &'a [(u8, u8, u8, u8)]) -> &'a [u8] {
        unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const _, data.len() * 4)
        }
    }

    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => {
            send_invalid_id(&channel);
            return Err(());
        }
    };
    if playback.video().is_none() {
        send(&channel, MediaEvent::NoStream { });
        return Err(());
    };
    if let Err(x) = playback.render_current_video_frame() {
        send_error!(&channel, x.to_string());
        return Err(());
    };

    let video = playback.video().unwrap();
    let current = video.current().unwrap();
    let frame = current.scaled.as_ref().unwrap();

    let pos = current.position;
    let time = f64::from(video.pos_timebase()) * pos as f64;
    let data = to_byte_slice(frame.plane(0));

    let mut binary = Vec::<u8>::new();
    binary.extend(pos.to_le_bytes().iter());
    binary.extend(time.to_le_bytes().iter());
    binary.extend(((frame.stride(0) / 4) as u64).to_le_bytes().iter());
    binary.extend((data.len() as u64).to_le_bytes().iter());
    binary.extend_from_slice(data);

    Ok(Response::new(InvokeResponseBody::Raw(binary)))
}

#[tauri::command]
pub fn seek_video(
    id: i32,
    position: i64,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel)
    };
    if playback.video().is_none() {
        return send(&channel, MediaEvent::NoStream { })
    };
    if let Err(e) = playback.seek_video_precise(position) {
        return send_error!(&channel, e.to_string());
    };

    send_done(&channel);
}

/** 
 * returns: [
 *  position    : i64
 *  time        : f64
 *  length      : u64
 *  sample_data : [f32]
 * ]
 * */ 
#[tauri::command]
pub fn send_current_audio_frame(
    id: i32,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) -> Result<ipc::Response, ()> {
    // FIXME: support multiple channels

    fn to_byte_slice<'a>(floats: &'a [f32]) -> &'a [u8] {
        unsafe {
            std::slice::from_raw_parts(floats.as_ptr() as *const _, floats.len() * 4)
        }
    }

    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => {
            send_invalid_id(&channel);
            return Err(());
        }
    };
    if playback.audio().is_none() {
        send(&channel, MediaEvent::NoStream { });
        return Err(());
    }

    let cxt = playback.audio().unwrap();
    let cached = match cxt.current() {
        Some(x) => x,
        None => {
            send_error!(&channel, "No current audio frame");
            return Err(());
        }
    };
    let pos = cached.position;
    let time = f64::from(cxt.pos_timebase()) * pos as f64;
    let data: &[f32] = cached.decoded.plane(0);

    let mut binary = Vec::<u8>::new();
    binary.extend(pos.to_le_bytes().iter());
    binary.extend(time.to_le_bytes().iter());
    binary.extend((data.len() as u64).to_le_bytes().iter());
    binary.extend_from_slice(&to_byte_slice(data));

    Ok(Response::new(InvokeResponseBody::Raw(binary)))
}

#[tauri::command]
pub fn get_intensities(
    id: i32,
    until: i64,
    step: i64,
    state: State<Mutex<PlaybackRegistry>>,
    channel: Channel<MediaEvent>,
) {
    let mut ap = state.lock().unwrap();
    let playback = match ap.table.get_mut(&id) {
        Some(x) => x,
        None => return send_invalid_id(&channel),
    };
    if playback.audio().is_none() {
        return send(&channel, MediaEvent::NoStream { });
    }

    let mut vector = Vec::<f32>::new();
    let mut counter = 0;
    let mut sum: f32 = 0.0;
    let mut start_position = -1;

    loop {
        if let Err(e) = playback.advance_to_next_audio_frame() {
            return send_error!(&channel, format!("Can't advance audio: {e}"));
        }
        let current = playback.audio().unwrap().current().unwrap();
        let data: &[f32] = current.decoded.plane(0);
        for sample in data {
            sum += (*sample) * (*sample);
            counter += 1;
            if counter == step {
                vector.push(sum / step as f32);
                counter = 0;
                sum = 0.0;
            }
        }
        if start_position < 0 {
            start_position = current.position;
        }
        if current.position >= until {
            break;
        }
    }

    return send(
        &channel,
        MediaEvent::IntensityList {
            start: start_position,
            end: start_position + (vector.len() as i64) * step,
            data: vector,
        },
    );
}
