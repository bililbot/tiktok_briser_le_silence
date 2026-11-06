use std::cell::RefCell;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    },
    Storage::Streams::DataReader,
    Win32::{
        Media::Audio::{
            eCapture, eConsole, eMultimedia, eRender, Endpoints::IAudioMeterInformation,
            IMMDeviceEnumerator, MMDeviceEnumerator,
        },
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
        UI::Input::KeyboardAndMouse::{
            keybd_event, GetAsyncKeyState, KEYEVENTF_KEYUP, VK_F10, VK_F11, VK_F12, VK_F8, VK_F9,
            VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
        },
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct MediaTrackInfo {
    pub connected: bool,
    pub playing: bool,
    pub source_app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover_b64: String,
    #[serde(skip)]
    pub cover_bytes: Vec<u8>,
}

thread_local! {
    static SESSION_MANAGER: RefCell<Option<GlobalSystemMediaTransportControlsSessionManager>> = RefCell::new(None);
}

fn get_or_create_manager() -> Option<GlobalSystemMediaTransportControlsSessionManager> {
    SESSION_MANAGER.with(|cell| {
        let mut opt = cell.borrow_mut();
        if let Some(ref m) = *opt {
            return Some(m.clone());
        }
        if let Ok(op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            if let Ok(mgr) = op.get() {
                *opt = Some(mgr.clone());
                return Some(mgr);
            }
        }
        None
    })
}

fn invalidate_manager() {
    SESSION_MANAGER.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Lit directement en Rust natif (WinRT) la musique ou vidéo en cours (YouTube, Spotify, VLC, Navigateur...)
/// Priorise automatiquement la session qui est en cours de lecture (`Playing`) et disposant d'un titre valide.
pub fn read_windows_media_session(
    last_title_key: &mut String,
    cached_b64: &mut String,
    cached_bytes: &mut Vec<u8>,
) -> MediaTrackInfo {
    let mut result = MediaTrackInfo::default();

    let Some(manager) = get_or_create_manager() else {
        return result;
    };

    let Ok(sessions) = manager.GetSessions() else {
        invalidate_manager();
        return result;
    };

    let count = sessions.Size().unwrap_or(0);
    if count == 0 {
        return result;
    }

    struct Candidate {
        _session: windows::Media::Control::GlobalSystemMediaTransportControlsSession,
        source_app: String,
        playing: bool,
        title: String,
        artist: String,
        album: String,
        props: windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
    }

    let mut playing_candidate: Option<Candidate> = None;
    let mut paused_candidate: Option<Candidate> = None;

    for i in 0..count {
        let Ok(s) = sessions.GetAt(i) else { continue };

        let Ok(props_op) = s.TryGetMediaPropertiesAsync() else { continue };
        let Ok(props) = props_op.get() else { continue };

        let title = props.Title().map(|s| s.to_string()).unwrap_or_default().trim().to_string();
        let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default().trim().to_string();
        let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default().trim().to_string();

        // Ignorer les sessions fantômes ou onglets muets sans métadonnées
        if title.is_empty() && artist.is_empty() {
            continue;
        }

        let playing = s
            .GetPlaybackInfo()
            .and_then(|pb| pb.PlaybackStatus())
            .map(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
            .unwrap_or(false);

        let source_app = s
            .SourceAppUserModelId()
            .map(|sa| sa.to_string())
            .unwrap_or_default();

        let cand = Candidate {
            _session: s,
            source_app,
            playing,
            title,
            artist,
            album,
            props,
        };

        if playing {
            playing_candidate = Some(cand);
            break; // Trouvé une session active en cours de lecture avec métadonnées !
        } else if paused_candidate.is_none() {
            paused_candidate = Some(cand);
        }
    }

    let Some(chosen) = playing_candidate.or(paused_candidate) else {
        return result;
    };

    result.connected = true;
    result.playing = chosen.playing;
    result.source_app = chosen.source_app;
    result.title = chosen.title;
    result.artist = chosen.artist;
    result.album = chosen.album;

    // Pochette / miniature YouTube ou Spotify
    let current_key = format!("{}||{}||{}", result.artist, result.title, result.album);
    if *last_title_key != current_key {
        *last_title_key = current_key;
        cached_b64.clear();
        cached_bytes.clear();

        if let Ok(thumb_ref) = chosen.props.Thumbnail() {
            if let Ok(open_op) = thumb_ref.OpenReadAsync() {
                if let Ok(stream) = open_op.get() {
                    let size = stream.Size().unwrap_or(0) as u32;
                    if size > 0 && size < 10_000_000 {
                        if let Ok(reader) = DataReader::CreateDataReader(&stream) {
                            if let Ok(load_op) = reader.LoadAsync(size) {
                                if load_op.get().is_ok() {
                                    let mut buf = vec![0u8; size as usize];
                                    if reader.ReadBytes(&mut buf).is_ok() {
                                        *cached_b64 = format!("data:image/jpeg;base64,{}", B64.encode(&buf));
                                        *cached_bytes = buf;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    result.cover_b64 = cached_b64.clone();
    result.cover_bytes = cached_bytes.clone();
    result
}

struct SystemAudioMeterInner {
    enumerator: Option<IMMDeviceEnumerator>,
    render_meter: Option<IAudioMeterInformation>,
    capture_meter: Option<IAudioMeterInformation>,
    last_reconnect: Instant,
}

impl SystemAudioMeterInner {
    fn new() -> Self {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut obj = Self {
            enumerator: None,
            render_meter: None,
            capture_meter: None,
            last_reconnect: Instant::now() - Duration::from_secs(5),
        };
        obj.reconnect();
        obj
    }

    fn reconnect(&mut self) {
        if self.last_reconnect.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_reconnect = Instant::now();

        unsafe {
            if self.enumerator.is_none() {
                if let Ok(enum_dev) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    self.enumerator = Some(enum_dev);
                }
            }

            if let Some(ref enum_dev) = self.enumerator {
                // 1. Sortie audio PC (Spotify, YouTube, streaming, sons média)
                let render_dev = enum_dev.GetDefaultAudioEndpoint(eRender, eMultimedia)
                    .or_else(|_| enum_dev.GetDefaultAudioEndpoint(eRender, eConsole)).ok();
                if let Some(device) = render_dev {
                    if let Ok(m) = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) {
                        self.render_meter = Some(m);
                    }
                }

                // 2. Entrée micro streamer (Voix, Micro Live, Casque/Micro)
                let capture_dev = enum_dev.GetDefaultAudioEndpoint(eCapture, eMultimedia)
                    .or_else(|_| enum_dev.GetDefaultAudioEndpoint(eCapture, eConsole)).ok();
                if let Some(device) = capture_dev {
                    if let Ok(m) = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) {
                        self.capture_meter = Some(m);
                    }
                }
            }
        }
    }

    fn get_peak(&mut self) -> f32 {
        if self.render_meter.is_none() || self.capture_meter.is_none() {
            self.reconnect();
        }

        let mut peak_render = 0.0_f32;
        let mut peak_capture = 0.0_f32;

        if let Some(ref m) = self.render_meter {
            unsafe {
                match m.GetPeakValue() {
                    Ok(peak) => peak_render = peak.clamp(0.0, 1.0),
                    Err(_) => {
                        self.render_meter = None;
                    }
                }
            }
        }

        if let Some(ref m) = self.capture_meter {
            unsafe {
                match m.GetPeakValue() {
                    Ok(peak) => peak_capture = peak.clamp(0.0, 1.0),
                    Err(_) => {
                        self.capture_meter = None;
                    }
                }
            }
        }

        // Détection combinée : son PC ou voix micro (avec gain de 1.4 sur le micro)
        let total_peak = f32::max(peak_render, (peak_capture * 1.4).min(1.0));
        total_peak
    }
}

/// Lit le niveau sonore réel (0.0 -> 1.0) qui sort de la carte son Windows (YouTube, Spotify, Vidéo, etc.)
/// Auto-reconnexion résiliente transparente en cas de changement de sortie audio.
pub struct SystemAudioMeter {
    inner: Mutex<SystemAudioMeterInner>,
}

impl SystemAudioMeter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(SystemAudioMeterInner::new()),
        }
    }

    pub fn get_peak(&self) -> f32 {
        if let Ok(mut inner) = self.inner.lock() {
            inner.get_peak()
        } else {
            0.0
        }
    }
}

pub fn trigger_media_play_pause() {
    unsafe {
        keybd_event(VK_MEDIA_PLAY_PAUSE.0 as u8, 0, Default::default(), 0);
        keybd_event(VK_MEDIA_PLAY_PAUSE.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub fn trigger_media_next() {
    unsafe {
        keybd_event(VK_MEDIA_NEXT_TRACK.0 as u8, 0, Default::default(), 0);
        keybd_event(VK_MEDIA_NEXT_TRACK.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub fn trigger_media_prev() {
    unsafe {
        keybd_event(VK_MEDIA_PREV_TRACK.0 as u8, 0, Default::default(), 0);
        keybd_event(VK_MEDIA_PREV_TRACK.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

/// Retourne Some("shield" | "vss" | "politique" | "musique" | "toggle_video") si F8 / F9 / F10 / F11 / F12 est pressé
pub fn check_global_hotkeys() -> Option<&'static str> {
    unsafe {
        if (GetAsyncKeyState(VK_F8.0 as i32) as u16 & 0x8000) != 0 {
            return Some("shield");
        }
        if (GetAsyncKeyState(VK_F9.0 as i32) as u16 & 0x8000) != 0 {
            return Some("vss");
        }
        if (GetAsyncKeyState(VK_F10.0 as i32) as u16 & 0x8000) != 0 {
            return Some("politique");
        }
        if (GetAsyncKeyState(VK_F11.0 as i32) as u16 & 0x8000) != 0 {
            return Some("musique");
        }
        if (GetAsyncKeyState(VK_F12.0 as i32) as u16 & 0x8000) != 0 {
            return Some("toggle_video");
        }
    }
    None
}
