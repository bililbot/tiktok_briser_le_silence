#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Module partagé avec la télécommande : ses commandes média / raccourcis ne servent qu'à main.rs
#[allow(dead_code)]
mod win_audio_media;

use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, RichText, Rounding, Sense, Stroke, TextureHandle, Vec2};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use win_audio_media::{read_windows_media_session, MediaTrackInfo, SystemAudioMeter};

struct SharedMusicState {
    track: MediaTrackInfo,
    audio_peak: f32,
    branch: String,
    custom_topic: String,
    box_mode: String, // "musique" | "sujet" | "combine"
}

struct MusicBoxApp {
    shared: Arc<Mutex<SharedMusicState>>,
    cover_tex: Option<TextureHandle>,
    last_tex_title: String,
    start_time: Instant,
    smoothed_bars: [f32; 28],
}

impl MusicBoxApp {
    fn new(_cc: &eframe::CreationContext<'_>, shared: Arc<Mutex<SharedMusicState>>) -> Self {
        Self {
            shared,
            cover_tex: None,
            last_tex_title: String::new(),
            start_time: Instant::now(),
            smoothed_bars: [0.1; 28],
        }
    }
}

impl eframe::App for MusicBoxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(33)); // ~30 FPS fluide, <0.5% CPU

        let (track, peak, branch, custom_topic, box_mode) = {
            let st = self.shared.lock().unwrap();
            (
                st.track.clone(),
                st.audio_peak,
                st.branch.clone(),
                st.custom_topic.clone(),
                st.box_mode.clone(),
            )
        };

        // Couleurs selon la branche active
        let (border_col, accent_col) = match branch.as_str() {
            "vss" => (Color32::from_rgb(168, 85, 247), Color32::from_rgb(203, 213, 225)),
            "politique" => (Color32::from_rgb(239, 68, 68), Color32::from_rgb(203, 213, 225)),
            _ => (Color32::from_rgb(249, 115, 22), Color32::from_rgb(203, 213, 225)),
        };

        // Charger la miniature YouTube / pochette Spotify en texture GPU si le titre a changé
        let tex_key = format!("{}||{}", track.artist, track.title);
        if tex_key != self.last_tex_title && !track.cover_bytes.is_empty() {
            self.last_tex_title = tex_key;
            if let Ok(img) = image::load_from_memory(&track.cover_bytes) {
                let rgba = img.resize_exact(120, 120, image::imageops::FilterType::Lanczos3).to_rgba8();
                let color_img = egui::ColorImage::from_rgba_unmultiplied([120, 120], rgba.as_raw());
                self.cover_tex = Some(ctx.load_texture("cover_art", color_img, egui::TextureOptions::LINEAR));
            }
        }

        // Cadre extérieur explicite pour TikTok LIVE Studio
        let frame_style = egui::Frame::none()
            .fill(Color32::from_rgb(14, 15, 20))
            .stroke(Stroke::new(4.5_f32, border_col))
            .inner_margin(egui::Margin::same(10.0));

        egui::CentralPanel::default().frame(frame_style).show(ctx, |ui| {
            // Double liseré or intérieur
            let rect = ui.max_rect();
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                Rounding::same(2.0),
                Stroke::new(1.5_f32, accent_col),
            );

            ui.horizontal(|ui| {
                ui.add_space(4.0);

                // --- GAUCHE : POCHETTE / MINIATURE YOUTUBE OU SPOTIFY ---
                let cov_size = Vec2::new(102.0, 102.0);
                let (cov_rect, _) = ui.allocate_exact_size(cov_size, Sense::hover());
                ui.painter().rect_filled(cov_rect, Rounding::same(4.0), Color32::from_rgb(24, 26, 34));

                if let Some(ref tex) = self.cover_tex {
                    ui.painter().image(
                        tex.id(),
                        cov_rect.shrink(2.0),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    // Disque vinyle par défaut
                    let center = cov_rect.center();
                    ui.painter().circle_stroke(center, 36.0, Stroke::new(2.5_f32, border_col));
                    ui.painter().circle_filled(center, 12.0, border_col);
                }
                ui.painter().rect_stroke(cov_rect, Rounding::same(4.0), Stroke::new(2.0_f32, border_col));

                ui.add_space(10.0);

                // --- DROITE : INFOS MUSIQUE (YOUTUBE / SPOTIFY) OU SUJET LIVE + SPECTRE RÉEL ---
                ui.vertical(|ui| {
                    let show_topic_primary = box_mode == "sujet" || (box_mode == "combine" && branch != "musique");

                    if show_topic_primary {
                        let branch_title = match branch.as_str() {
                            "vss" => "PAROLE AUX SURVIVANT·E·S — INCESTE & VSS",
                            "politique" => "FRONT MILITANT — ANTIFA • FÉMINISTE • ANTICAPITALISTE",
                            _ => "UNDERGROUND INSURRECTION — LIVE",
                        };
                        ui.label(
                            RichText::new(branch_title)
                                .font(FontId::new(18.5, FontFamily::Proportional))
                                .strong()
                                .color(Color32::WHITE),
                        );

                        let topic_txt = if !custom_topic.is_empty() {
                            custom_topic.clone()
                        } else if branch == "vss" {
                            "Écoute & Soutien • Aide 24h/24 : 119 (Enfance) | 3919 (Violences)".to_string()
                        } else {
                            "Débat militant & Riposte populaire".to_string()
                        };
                        ui.label(
                            RichText::new(topic_txt)
                                .font(FontId::new(13.5, FontFamily::Proportional))
                                .strong()
                                .color(accent_col),
                        );

                        if track.connected && !track.title.is_empty() {
                            ui.label(
                                RichText::new(format!("🎵 Son en cours : {} — {}", track.title, track.artist))
                                    .font(FontId::new(11.5, FontFamily::Proportional))
                                    .color(Color32::from_rgb(209, 213, 219)),
                            );
                        }
                    } else if track.connected {
                        // Affichage pur Musique (YouTube, Spotify, etc. SANS mentionner le nom de l'appli) - Police XL Haute Lisibilité
                        ui.label(
                            RichText::new(track.title.to_uppercase())
                                .font(FontId::new(23.0, FontFamily::Proportional))
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new(format!("🎤 {}", track.artist.to_uppercase()))
                                .font(FontId::new(16.0, FontFamily::Proportional))
                                .strong()
                                .color(accent_col),
                        );
                        let alb = if track.album.is_empty() {
                            "🔊 SESSION LIVE".to_string()
                        } else {
                            format!("💿 ALBUM : {}", track.album.to_uppercase())
                        };
                        ui.label(
                            RichText::new(alb)
                                .font(FontId::new(13.5, FontFamily::Proportional))
                                .strong()
                                .color(Color32::from_rgb(240, 242, 245)),
                        );
                    } else {
                        ui.label(
                            RichText::new("UNDERGROUND INSURRECTION")
                                .font(FontId::new(20.0, FontFamily::Proportional))
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new("🎤 EN ATTENTE DE SON (YOUTUBE / SPOTIFY / PC)")
                                .font(FontId::new(13.0, FontFamily::Proportional))
                                .color(accent_col),
                        );
                        ui.label(
                            RichText::new("Lance une musique ou une vidéo sur ton PC")
                                .font(FontId::new(11.5, FontFamily::Proportional))
                                .color(Color32::from_rgb(160, 165, 175)),
                        );
                    }

                    ui.add_space(4.0);

                    // --- BARRES DE SPECTRE AUDIO RÉACTIVES AU VRAI SON DU PC (WASAPI) ---
                    let avail_w = ui.available_width().max(200.0);
                    let spec_h = 24.0;
                    let (spec_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, spec_h), Sense::hover());
                    ui.painter().rect_filled(spec_rect, Rounding::same(3.0), Color32::from_rgb(9, 10, 14));

                    let bar_count = 28;
                    let spacing = 3.0;
                    let bar_w = ((avail_w - (bar_count as f32 * spacing)) / bar_count as f32).max(4.0);
                    let t = self.start_time.elapsed().as_secs_f32() * 11.0;

                    for i in 0..bar_count {
                        let target = if peak > 0.015 {
                            // Vrai son détecté sur la carte son Windows !
                            let freq_mod = ((t + i as f32 * 0.55).sin().abs() * 0.65)
                                + ((t * 1.4 - i as f32 * 0.35).cos().abs() * 0.35);
                            (peak * 1.35 * freq_mod).clamp(0.12, 1.0)
                        } else if track.playing {
                            let freq_mod = ((t + i as f32 * 0.45).sin().abs() * 0.6)
                                + ((t * 1.2 - i as f32 * 0.3).cos().abs() * 0.4);
                            (0.75 * freq_mod).clamp(0.12, 0.95)
                        } else {
                            0.12 + ((t * 0.25 + i as f32 * 0.4).sin().abs() * 0.08)
                        };

                        self.smoothed_bars[i] = self.smoothed_bars[i] * 0.55 + target * 0.45;
                        let bh = (self.smoothed_bars[i] * (spec_h - 4.0)).max(3.0);

                        let x1 = spec_rect.left() + 2.0 + i as f32 * (bar_w + spacing);
                        let y1 = spec_rect.bottom() - bh;
                        let bar_rect = Rect::from_min_max(Pos2::new(x1, y1), Pos2::new(x1 + bar_w, spec_rect.bottom()));

                        let col = if self.smoothed_bars[i] > 0.65 { accent_col } else { border_col };
                        ui.painter().rect_filled(bar_rect, Rounding::same(1.5), col);
                    }
                });
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let shared = Arc::new(Mutex::new(SharedMusicState {
        track: MediaTrackInfo::default(),
        audio_peak: 0.0,
        branch: "musique".to_string(),
        custom_topic: String::new(),
        box_mode: "musique".to_string(),
    }));

    // Thread 1 : Lecture native WinRT (YouTube, Spotify, VLC...) + Lecture état branche
    let shared_media = Arc::clone(&shared);
    thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let mut last_key = String::new();
        let mut cached_b64 = String::new();
        let mut cached_bytes = Vec::new();
        loop {
            let info = read_windows_media_session(&mut last_key, &mut cached_b64, &mut cached_bytes);
            if let Ok(mut st) = shared_media.lock() {
                st.track = info;
            }
            // Lire aussi live_state.json si la Télécommande a changé la branche
            if let Ok(raw) = std::fs::read_to_string("live_state.json") {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Ok(mut st) = shared_media.lock() {
                        if let Some(b) = val.get("branch").and_then(|v| v.as_str()) {
                            st.branch = b.to_string();
                        }
                        if let Some(t) = val.get("custom_topic").and_then(|v| v.as_str()) {
                            st.custom_topic = t.to_string();
                        }
                        if let Some(m) = val.get("box_mode").and_then(|v| v.as_str()) {
                            st.box_mode = m.to_string();
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
    });

    // Thread 2 : Lecture temps réel du niveau sonore de la carte son PC (WASAPI)
    let shared_audio = Arc::clone(&shared);
    thread::spawn(move || {
        let meter = SystemAudioMeter::new();
        let mut rolling_peak: f32 = 0.0;
        loop {
            let peak = meter.get_peak();
            if peak > rolling_peak {
                rolling_peak = peak;
            } else {
                rolling_peak = (rolling_peak * 0.94).max(0.0);
            }
            if let Ok(mut st) = shared_audio.lock() {
                st.audio_peak = rolling_peak;
            }
            thread::sleep(Duration::from_millis(35));
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MUSIQUE_LIVE_TIKTOK")
            .with_inner_size([560.0, 134.0])
            .with_min_inner_size([420.0, 120.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MUSIQUE_LIVE_TIKTOK",
        options,
        Box::new(|cc| Ok(Box::new(MusicBoxApp::new(cc, shared)))),
    )
}
