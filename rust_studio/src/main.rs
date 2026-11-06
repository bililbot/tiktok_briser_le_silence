#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod win_audio_media;

use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Rounding, Stroke};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Response, Server};
use win_audio_media::{
    global_hotkeys_down, read_windows_media_session, trigger_media_next,
    trigger_media_play_pause, trigger_media_prev, MediaTrackInfo, SystemAudioMeter,
};

const CREATE_NO_WINDOW: u32 = 0x08000000;
const LIBRARY_FILE: &str = "video_library.json";
const EMERGENCY_FILE: &str = "emergency_numbers.json";
const BOT_CONFIG_FILE: &str = "bot_config.json";
const JINGLES_FILE: &str = "jingles_library.json";
const PREFERENCES_FILE: &str = "studio_preferences.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JingleItem {
    pub id: String,
    pub label: String,
    pub file_path: String,
    pub media_type: String, // "audio" | "video" | "synth"
    pub volume: u32,        // 0..=100
}

fn default_jingles() -> Vec<JingleItem> {
    vec![
        JingleItem { id: "alerte".into(), label: "🚨 Alerte".into(), file_path: "".into(), media_type: "synth".into(), volume: 85 },
        JingleItem { id: "soutien".into(), label: "💜 Soutien".into(), file_path: "".into(), media_type: "synth".into(), volume: 80 },
        JingleItem { id: "transition".into(), label: "📢 Transition".into(), file_path: "".into(), media_type: "synth".into(), volume: 75 },
        JingleItem { id: "bip".into(), label: "🔇 Bip Censure".into(), file_path: "".into(), media_type: "synth".into(), volume: 90 },
        JingleItem { id: "gong".into(), label: "✊ Gong Militant".into(), file_path: "".into(), media_type: "synth".into(), volume: 80 },
        JingleItem { id: "bass".into(), label: "🎵 Drop Bass".into(), file_path: "".into(), media_type: "synth".into(), volume: 85 },
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BotCustomTrigger {
    pub enabled: bool,
    pub keywords: String,       // Ex: "!aide, aide, secours, plainte"
    pub action: String,         // "emergency" | "jingle" | "pin" | "tts"
    pub action_param: String,   // Ex: "alerte" ou texte
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BotConfig {
    pub auto_emergency: bool,
    pub auto_pin_questions: bool,
    pub tts_voice_enabled: bool,
    pub tts_read_pinned: bool,
    pub filter_blacklist: bool,
    pub blacklist_words: String,
    pub triggers: Vec<BotCustomTrigger>,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            auto_emergency: true,
            auto_pin_questions: false,
            tts_voice_enabled: false,
            tts_read_pinned: true,
            filter_blacklist: true,
            blacklist_words: "fdp, tg, ntm, connard, salope, batard, connerie".to_string(),
            triggers: vec![
                BotCustomTrigger {
                    enabled: true,
                    keywords: "aide, urgence, secours, 119, 3919, plainte, subi".to_string(),
                    action: "emergency".to_string(),
                    action_param: "Grand Panneau Urgences".to_string(),
                },
                BotCustomTrigger {
                    enabled: true,
                    keywords: "!question, !temoignage".to_string(),
                    action: "pin".to_string(),
                    action_param: "".to_string(),
                },
                BotCustomTrigger {
                    enabled: true,
                    keywords: "!alerte, !sirene".to_string(),
                    action: "jingle".to_string(),
                    action_param: "alerte".to_string(),
                },
                BotCustomTrigger {
                    enabled: true,
                    keywords: "!soutien, !force, !coeur".to_string(),
                    action: "jingle".to_string(),
                    action_param: "soutien".to_string(),
                },
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SavedVideoLibrary {
    pub folder_path: String,
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmergencyNumberItem {
    pub enabled: bool,
    pub number: String,
    pub label: String,
}

pub fn default_emergency_numbers() -> Vec<EmergencyNumberItem> {
    vec![
        EmergencyNumberItem {
            enabled: true,
            number: "119".to_string(),
            label: "ENFANCE EN DANGER (Gratuit & Anonyme 24h/24)".to_string(),
        },
        EmergencyNumberItem {
            enabled: true,
            number: "3919".to_string(),
            label: "VIOLENCES FEMMES INFO (Écoute & Orientation 24h/24)".to_string(),
        },
        EmergencyNumberItem {
            enabled: true,
            number: "0 800 05 95 95".to_string(),
            label: "COLLECTIF FÉMINISTE CONTRE LE VIOL (Soutien VSS)".to_string(),
        },
        EmergencyNumberItem {
            enabled: true,
            number: "114".to_string(),
            label: "URGENCE PAR SMS (Si vous ne pouvez pas parler)".to_string(),
        },
        EmergencyNumberItem {
            enabled: true,
            number: "3114".to_string(),
            label: "PRÉVENTION DÉTRESSE & SUICIDE (Gratuit 24h/24)".to_string(),
        },
    ]
}

/// Message du chat contenant un mot-clé d'aide, à traiter en privé après le live
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct HelpAlert {
    pub id: u64,
    pub ts: f64,
    #[serde(default)]
    pub time_label: String, // heure locale "HH:MM" à la réception
    pub author: String,
    pub handle: String,
    pub text: String,
    pub handled: bool,
}

/// Heure locale Windows au format "HH:MM" (gère l'heure d'été)
fn local_time_label() -> String {
    local_datetime().1
}

/// Date et heure locales Windows : ("AAAA-MM-JJ", "HH:MM")
fn local_datetime() -> (String, String) {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    (
        format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay),
        format!("{:02}:{:02}", t.wHour, t.wMinute),
    )
}

/// Ligne de la chronologie du journal de live
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct JournalEvent {
    pub time_label: String,
    pub text: String,
}

/// Journal du live en cours : compteurs + chronologie, exporté en Markdown par « Terminer le live »
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LiveJournal {
    pub start_ts: f64,
    pub start_date: String,
    pub start_time: String,
    pub help_seq_at_start: u64,
    pub comments: u64,
    pub gifts: u64,
    pub shares: u64,
    pub follows: u64,
    pub events: Vec<JournalEvent>,
}

/// (Re)démarre le journal à l'instant présent
fn journal_reset(st: &mut FullLiveState) {
    let (date, time) = local_datetime();
    st.journal = LiveJournal {
        start_ts: now_secs(),
        start_date: date,
        start_time: time,
        help_seq_at_start: st.help_alert_seq,
        ..Default::default()
    };
}

fn journal_log(st: &mut FullLiveState, text: String) {
    st.journal.events.push(JournalEvent { time_label: local_time_label(), text });
    if st.journal.events.len() > 500 {
        st.journal.events.remove(0);
    }
}

/// Écrit le journal en Markdown dans journaux/ puis repart sur un journal vierge
fn export_journal(st: &mut FullLiveState) -> Result<PathBuf, String> {
    let j = &st.journal;
    let (end_date, end_time) = local_datetime();
    let mins = ((now_secs() - j.start_ts).max(0.0) / 60.0) as u64;
    let mut md = format!(
        "# 📝 Journal du live — {} {}\n\n- **Début** : {} {}\n- **Fin** : {} {}\n- **Durée** : {} h {:02} min\n\n",
        j.start_date, j.start_time, j.start_date, j.start_time, end_date, end_time, mins / 60, mins % 60
    );
    let helps: Vec<&HelpAlert> = st.help_alerts.iter().filter(|a| a.id > j.help_seq_at_start).collect();
    md.push_str(&format!(
        "## 📊 Chiffres\n\n| | |\n|:--|--:|\n| Commentaires reçus | {} |\n| Demandes d'aide | {} |\n| Cadeaux | {} |\n| Partages | {} |\n| Follows | {} |\n\n",
        j.comments, helps.len(), j.gifts, j.shares, j.follows
    ));
    md.push_str("## 🕒 Chronologie\n\n");
    if j.events.is_empty() {
        md.push_str("_Aucun événement noté._\n");
    }
    for e in &j.events {
        md.push_str(&format!("- **{}** — {}\n", e.time_label, e.text));
    }
    let p = &st.poll;
    if !p.question.trim().is_empty() {
        md.push_str(&format!("\n## 🗳️ Dernier sondage\n\n**{}**\n\n", p.question));
        for (c, v) in [(&p.choice1, p.votes1), (&p.choice2, p.votes2), (&p.choice3, p.votes3), (&p.choice4, p.votes4)] {
            if !c.trim().is_empty() {
                md.push_str(&format!("- {} : {} vote(s)\n", c, v));
            }
        }
    }
    md.push_str("\n## 🆘 Demandes d'aide pendant ce live\n\n> Privé — ne pas partager. Pour recontacter les personnes.\n\n");
    if helps.is_empty() {
        md.push_str("_Aucune._\n");
    }
    for a in &helps {
        let who = if a.handle.is_empty() { a.author.clone() } else { format!("{} {}", a.author, a.handle) };
        let status = if a.handled { "✅ traité" } else { "⏳ à traiter" };
        md.push_str(&format!("- **{}** — {} ({}) : {}\n", a.time_label, who, status, a.text.replace('\n', " ")));
    }

    let dir = app_path("journaux");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("live_{}_{}.md", j.start_date, j.start_time.replace(':', "h")));
    std::fs::write(&file, md).map_err(|e| e.to_string())?;
    journal_reset(st);
    Ok(file)
}

/// Remerciement à afficher sur les scènes (cadeau, partage ou follow reçu via le pont chat)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ThanksItem {
    pub id: u64,
    pub kind: String, // "gift" | "share" | "follow"
    pub author: String,
    pub gift: String,
    pub count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ChatCommentItem {
    pub author: String,
    pub handle: String,
    pub text: String,
    pub avatar: String,
    pub is_question: bool,
    pub is_help_trigger: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SongRequestItem {
    pub id: u64,
    pub author: String,
    pub handle: String,
    pub query: String,
    pub timestamp: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SongVoteState {
    pub active: bool,
    pub title: String,
    pub choice1: String,
    pub choice2: String,
    pub choice3: String,
    pub choice4: String,
    pub votes1: u32,
    pub votes2: u32,
    pub votes3: u32,
    pub votes4: u32,
    pub voters: Vec<String>,
    pub winner: Option<u8>,
}

impl Default for SongVoteState {
    fn default() -> Self {
        Self {
            active: false,
            title: "Quel est le prochain son ? Tapez !1, !2 ou !3".to_string(),
            choice1: "Bérurier Noir - Salut à toi".to_string(),
            choice2: "Tagada Jones - Mort aux cons".to_string(),
            choice3: "Keny Arkana - La Rage".to_string(),
            choice4: String::new(),
            votes1: 0,
            votes2: 0,
            votes3: 0,
            votes4: 0,
            voters: Vec::new(),
            winner: None,
        }
    }
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CustomBackgrounds {
    pub vss_9_16: String,
    pub politique_9_16: String,
    pub musique_9_16: String,
    pub chill_9_16: String,
    pub vss_16_9: String,
    pub politique_16_9: String,
    pub musique_16_9: String,
    pub chill_16_9: String,
}

impl Default for CustomBackgrounds {
    fn default() -> Self {
        Self {
            vss_9_16: "/assets/bg_branche1_vss.jpg".to_string(),
            politique_9_16: "/assets/bg_branche2_politique.jpg".to_string(),
            musique_9_16: "/assets/bg_branche3_musique.jpg".to_string(),
            chill_9_16: "/assets/bg_branche4_chill.jpg".to_string(),
            vss_16_9: "/assets/bg_branche1_vss_wide16_9.jpg".to_string(),
            politique_16_9: "/assets/bg_branche2_politique_wide16_9.jpg".to_string(),
            musique_16_9: "/assets/bg_branche3_musique_wide16_9.jpg".to_string(),
            chill_16_9: "/assets/bg_branche4_chill_wide16_9.jpg".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TapperConfig {
    pub enabled: bool,
    pub mode: String, // "post_message" | "webview"
    pub clicks_per_second: u32,
    pub jitter_enabled: bool,
    pub random_pauses: bool,
    pub target_window_title: String,
}

impl Default for TapperConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "post_message".to_string(),
            clicks_per_second: 4,
            jitter_enabled: true,
            random_pauses: true,
            target_window_title: "TikTok".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LikeGoalConfig {
    pub active: bool,
    pub current_likes: u64,
    pub target_likes: u64,
    pub title: String,
}

impl Default for LikeGoalConfig {
    fn default() -> Self {
        Self {
            active: true,
            current_likes: 1250,
            target_likes: 5000,
            title: "🎯 OBJECTIF TAPOTEMENTS : 5 000 LIKES".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PollState {
    pub active: bool,
    pub question: String,
    pub choice1: String,
    pub choice2: String,
    pub choice3: String,
    pub choice4: String,
    pub votes1: u32,
    pub votes2: u32,
    pub votes3: u32,
    pub votes4: u32,
    pub voters: Vec<String>,
    pub winner: String,
}

impl Default for PollState {
    fn default() -> Self {
        Self {
            active: false,
            question: "Quel sujet aborder en priorité ?".to_string(),
            choice1: "Imprescriptibilité des crimes".to_string(),
            choice2: "Justice sociale & aides".to_string(),
            choice3: "Musique & dédicaces".to_string(),
            choice4: "Débat libre & Q/A".to_string(),
            votes1: 0,
            votes2: 0,
            votes3: 0,
            votes4: 0,
            voters: Vec::new(),
            winner: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArticleState {
    pub visible: bool,
    pub title: String,
    pub source: String,
    pub date: String,
    pub text: String,
    pub image_url: String,
    pub show_image: bool,
    pub font_size_title: u32,
    pub font_size_text: u32,
    pub text_align: String,
    pub text_color: String,
    pub title_color: String,
    pub source_badge_color: String,
    pub bg_color: String,
    pub bg_opacity: u32,
    pub border_color: String,
    pub border_width: u32,
    pub border_radius: u32,
    pub glow: bool,
    pub auto_scroll: bool,
    pub scroll_speed: u32,
    pub theme: String,
}

impl Default for ArticleState {
    fn default() -> Self {
        Self {
            visible: false,
            title: "REVUE DE PRESSE & ACTUALITÉ".to_string(),
            source: "📰 PRESSE & MÉDIAS".to_string(),
            date: "".to_string(),
            text: "Collez l'URL d'un article ou copiez votre texte dans la Télécommande pour l'afficher en direct sur le live.".to_string(),
            image_url: "".to_string(),
            show_image: true,
            font_size_title: 22,
            font_size_text: 16,
            text_align: "left".to_string(),
            text_color: "#f8fafc".to_string(),
            title_color: "#fbbf24".to_string(),
            source_badge_color: "#dc2626".to_string(),
            bg_color: "#0f172a".to_string(),
            bg_opacity: 92,
            border_color: "#f59e0b".to_string(),
            border_width: 2,
            border_radius: 12,
            glow: true,
            auto_scroll: false,
            scroll_speed: 3,
            theme: "presse".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FullLiveState {
    pub branch: String,
    pub branch_timestamp: f64,
    pub box_mode: String, // "combine" | "musique" | "sujet"
    pub custom_topic: String,
    pub custom_sub: String,
    pub audio_peak: f32,
    pub spotify: MediaTrackInfo,
    // Numéros d'urgence & d'écoute (Ultra-visibles en mode Inceste & VSS)
    pub emergency_visible: bool,
    pub emergency_title: String,
    pub emergency_style: String, // "grille" | "zoom"
    pub emergency_scale_pct: u32,
    pub emergency_pos_16_9: String, // "left" | "right" | "top"
    pub emergency_numbers: Vec<EmergencyNumberItem>,
    // Bandeau Trigger Warning (Avertissement Sujet Sensible)
    pub trigger_warning_visible: bool,
    pub trigger_warning_text: String,
    // Commentaire / Question épinglé(e) sur le Live + Bot Chat TikTok
    // Bouclier anti-raid : masque le chat/épinglé, gèle les déclencheurs du bot et affiche un écran de pause
    #[serde(default)]
    pub shield_active: bool,
    #[serde(default)]
    pub shield_text: String,
    // Chrono de débat : temps de parole affiché sur les scènes, gong automatique à la fin
    #[serde(default)]
    pub timer_active: bool,
    #[serde(default)]
    pub timer_running: bool,
    #[serde(default)]
    pub timer_label: String,
    #[serde(default)]
    pub timer_total_secs: u32,
    #[serde(default)]
    pub timer_end_ts: f64, // fin prévue (epoch s), valable si timer_running
    #[serde(default)]
    pub timer_remaining_secs: f64, // temps restant figé, valable en pause
    #[serde(default)]
    pub timer_gong_done: bool,
    // Remerciements automatiques : les scènes affichent les éléments dont l'id est plus récent que le dernier vu
    // File privée des demandes d'aide (jamais affichée sur les scènes) pour recontacter les personnes après le live
    #[serde(default)]
    pub journal: LiveJournal,
    #[serde(default)]
    pub help_alert_seq: u64,
    #[serde(default)]
    pub help_alerts: Vec<HelpAlert>,
    #[serde(default = "default_true")]
    pub thanks_enabled: bool,
    #[serde(default)]
    pub thanks_seq: u64,
    #[serde(default)]
    pub thanks_queue: Vec<ThanksItem>,
    pub pinned_visible: bool,
    pub pinned_author: String,
    pub pinned_text: String,
    pub pinned_avatar: String,
    pub pinned_font_size: u32,
    pub pinned_align: String, // "left" | "center" | "right"
    pub chat_handle: String,
    pub chat_status: String,
    pub chat_comments: Vec<ChatCommentItem>,
    pub bot_config: BotConfig,
    pub bot_auto_emergency: bool,
    pub bot_auto_pin_questions: bool,
    // Boîte à Jingles / Soundboard
    pub jingle_cmd_id: u64,
    pub jingle_type: String, // "synth" | "audio" | "video"
    pub jingle_key: String,
    pub jingle_custom_path: String,
    pub jingle_file_path: String,
    pub jingle_volume: u32,
    pub custom_jingles: Vec<JingleItem>,
    // Nouveau Module Encart Texte Libre
    pub custom_box_visible: bool,
    pub custom_box_has_title: bool,
    pub custom_box_title: String,
    pub custom_box_text: String,
    pub custom_box_font_size: u32,
    pub custom_box_text_color: String,
    pub custom_box_align: String,
    pub custom_box_bg_color: String,
    pub custom_box_bg_opacity: u32,
    pub custom_box_border_color: String,
    pub custom_box_border_width: u32,
    pub custom_box_border_radius: u32,
    pub custom_box_padding: u32,
    #[serde(default = "default_22")]
    pub custom_box_title_font_size: u32,
    #[serde(default = "default_22")]
    pub custom_topic_font_size: u32,
    #[serde(default = "default_14")]
    pub custom_sub_font_size: u32,
    #[serde(default)]
    pub article: ArticleState,
    // Paramètres Vidéo Témoignages & Reportages
    pub video_active: bool,
    pub video_scale_pct: u32, // 8% (Mini vignette) à 60%
    pub video_title: String,
    pub video_source_type: String, // "local" | "youtube"
    pub video_file_path: String,
    pub video_youtube_id: String,
    pub video_anti_detect: bool,
    pub video_volume: u32,
    pub video_cmd_id: u64,
    pub video_cmd_action: String,
    #[serde(default = "default_true")]
    pub video_yt_autoplay: bool,
    #[serde(default)]
    pub video_yt_loop: bool,
    #[serde(default = "default_true")]
    pub video_yt_controls: bool,
    #[serde(default)]
    pub video_yt_mute: bool,
    pub preferred_stage_format: String, // "9_16" | "16_9"
    // Jukebox & Votes du Chat TikTok
    pub song_mode: String, // "off" | "queue" | "vote"
    pub song_requests: Vec<SongRequestItem>,
    pub current_song_requester: String,
    pub song_vote: SongVoteState,
    #[serde(default)]
    pub custom_backgrounds: CustomBackgrounds,
    #[serde(default)]
    pub tapper_config: TapperConfig,
    #[serde(default)]
    pub like_goal: LikeGoalConfig,
    #[serde(default)]
    pub poll: PollState,
    #[serde(default = "default_tiktok_host")]
    pub tiktok_host_account: String,
    #[serde(default = "default_tiktok_helper")]
    pub tiktok_helper_account: String,
}

fn default_tiktok_host() -> String { "@lebyb".to_string() }
fn default_tiktok_helper() -> String { "@second_compte".to_string() }

fn default_true() -> bool { true }

/// Ajoute les polices emoji/symboles de Windows en secours : sans elles, egui affiche « □ »
/// à la place de 🛡️ ⏱️ 🗳️… (les glyphes sont rendus en monochrome)
fn install_windows_emoji_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, file) in [("segoe_emoji", r"C:\Windows\Fonts\seguiemj.ttf"), ("segoe_symbol", r"C:\Windows\Fonts\seguisym.ttf")] {
        if let Ok(bytes) = std::fs::read(file) {
            fonts.font_data.insert(name.to_string(), egui::FontData::from_owned(bytes));
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.to_string());
            }
        }
    }
    ctx.set_fonts(fonts);
}

/// Bascule de branche (boutons de la régie et raccourcis F9–F11)
fn apply_branch(st: &mut FullLiveState, branch_key: &str) {
    st.branch = branch_key.to_string();
    st.branch_timestamp = now_secs();
    match branch_key {
        "vss" => {
            st.custom_topic = "PAROLE AUX SURVIVANT·E·S : BRISER LE TABOU DE L'INCESTE".to_string();
            st.custom_sub = "📞 Aide 24h/24 : 119 (Enfance) • 3919 (Violences) • Chat bienveillant".to_string();
            st.box_mode = "combine".to_string();
            // Activer automatiquement le grand panneau des numéros d'urgence en mode Inceste & VSS !
            st.emergency_visible = true;
        }
        "politique" => {
            st.custom_topic = "FRONT MILITANT : ANTIFA • FÉMINISTE • ANTICAPITALISTE".to_string();
            st.custom_sub = "✊ Justice sociale, égalité & riposte populaire • Débat en direct".to_string();
            st.box_mode = "combine".to_string();
            st.emergency_visible = false;
        }
        "chill" => {
            st.custom_topic = "☕ CHILL & DÉTENTE : ON PREND LE TEMPS".to_string();
            st.custom_sub = "Discussion posée • Écoute & bienveillance • Vos questions en direct".to_string();
            st.box_mode = "combine".to_string();
            st.emergency_visible = false;
        }
        _ => {
            st.custom_topic = "UNDERGROUND INSURRECTION : LA BANDE-SON DE NOS LUTTES".to_string();
            st.custom_sub = "🔊 Punk • Techno • Hardcore • Hip-Hop • Metal • Grime • Dub".to_string();
            st.box_mode = "musique".to_string();
            st.emergency_visible = false;
        }
    }
}

/// Lance un jingle de la bibliothèque (synthé par défaut si l'id n'y est pas)
fn fire_jingle(st: &mut FullLiveState, jingle_id: &str) {
    let item = st.custom_jingles.iter().find(|j| j.id == jingle_id).cloned();
    st.jingle_cmd_id += 1;
    match item {
        Some(item) => {
            st.jingle_type = item.media_type;
            st.jingle_key = item.id;
            st.jingle_file_path = item.file_path;
            st.jingle_volume = item.volume;
        }
        None => {
            st.jingle_type = "synth".to_string();
            st.jingle_key = jingle_id.to_string();
            st.jingle_file_path = String::new();
            st.jingle_volume = 80;
        }
    }
}

/// Exécute l'action d'un raccourci global (F1–F4, F8–F12)
fn apply_hotkey(st: &mut FullLiveState, action: &str) {
    match action {
        "alerte" => fire_jingle(st, "alerte"),
        "pin_question" => {
            // F2 : épingle la dernière question du chat, ou retire l'épingle si une est affichée
            if st.pinned_visible {
                st.pinned_visible = false;
            } else if !st.shield_active {
                if let Some(q) = st.chat_comments.iter().rev().find(|c| c.is_question).cloned() {
                    st.pinned_author = q.author;
                    st.pinned_text = q.text;
                    st.pinned_avatar = q.avatar;
                    st.pinned_visible = true;
                }
            }
        }
        "emergency" => st.emergency_visible = !st.emergency_visible,
        "annonce" => st.custom_box_visible = !st.custom_box_visible,
        "shield" => {
            st.shield_active = !st.shield_active;
            st.branch_timestamp = now_secs();
        }
        "vss" | "politique" | "musique" => apply_branch(st, action),
        "toggle_video" => {
            st.video_active = !st.video_active;
            st.branch_timestamp = now_secs();
        }
        _ => return,
    }
    save_state_json(st);
}

/// Ajoute un remerciement à la file (20 derniers gardés, les scènes ne rejouent que les nouveaux)
fn push_thanks(st: &mut FullLiveState, kind: &str, author: String, gift: String, count: u32) {
    st.thanks_seq += 1;
    let id = st.thanks_seq;
    st.thanks_queue.push(ThanksItem { id, kind: kind.to_string(), author, gift, count });
    if st.thanks_queue.len() > 20 {
        st.thanks_queue.remove(0);
    }
}
fn default_22() -> u32 { 22 }
fn default_14() -> u32 { 14 }

impl Default for FullLiveState {
    fn default() -> Self {
        Self {
            branch: "musique".to_string(),
            branch_timestamp: now_secs(),
            box_mode: "combine".to_string(),
            custom_topic: "SONS ENGAGÉS : PUNK • TECHNO • HARDCORE • HIP-HOP".to_string(),
            custom_sub: "Aide 24h/24 : 119 (Enfance) • 3919 (Violences) • #BriserLeSilence".to_string(),
            audio_peak: 0.0,
            spotify: MediaTrackInfo::default(),
            emergency_visible: false,
            emergency_title: "🚨 NUMÉROS D'AIDE & D'ÉCOUTE (GRATUITS & ANONYMES)".to_string(),
            emergency_style: "grille".to_string(),
            emergency_scale_pct: 100,
            emergency_pos_16_9: "top".to_string(),
            emergency_numbers: default_emergency_numbers(),
            trigger_warning_visible: false,
            trigger_warning_text: "⚠️ TW : Sujet sensible (Inceste & Violences Sexuelles) — Prenez soin de vous".to_string(),
            shield_active: false,
            shield_text: String::new(),
            timer_active: false,
            timer_running: false,
            timer_label: String::new(),
            timer_total_secs: 0,
            timer_end_ts: 0.0,
            timer_remaining_secs: 0.0,
            timer_gong_done: false,
            journal: LiveJournal::default(),
            help_alert_seq: 0,
            help_alerts: Vec::new(),
            thanks_enabled: true,
            thanks_seq: 0,
            thanks_queue: Vec::new(),
            pinned_visible: false,
            pinned_author: String::new(),
            pinned_text: String::new(),
            pinned_avatar: String::new(),
            pinned_font_size: 18,
            pinned_align: "left".to_string(),
            chat_handle: "Et si ...".to_string(),
            chat_status: " Prêt à connecter au Live TikTok".to_string(),
            chat_comments: Vec::new(),
            bot_config: BotConfig::default(),
            bot_auto_emergency: true,
            bot_auto_pin_questions: false,
            jingle_cmd_id: 0,
            jingle_type: String::new(),
            jingle_key: String::new(),
            jingle_custom_path: String::new(),
            jingle_file_path: String::new(),
            jingle_volume: 80,
            custom_jingles: default_jingles(),
            custom_box_visible: false,
            custom_box_has_title: true,
            custom_box_title: "📢 ANNONCE DU LIVE".to_string(),
            custom_box_text: "Bienvenue sur le Live !\nPartagez le stream et respectez les règles du chat.".to_string(),
            custom_box_font_size: 20,
            custom_box_text_color: "#ffffff".to_string(),
            custom_box_align: "center".to_string(),
            custom_box_bg_color: "#121420".to_string(),
            custom_box_bg_opacity: 90,
            custom_box_border_color: "#a855f7".to_string(),
            custom_box_border_width: 2,
            custom_box_border_radius: 10,
            custom_box_padding: 12,
            custom_box_title_font_size: 22,
            custom_topic_font_size: 22,
            custom_sub_font_size: 14,
            article: ArticleState::default(),
            video_active: false,
            video_scale_pct: 24,
            video_title: "🎥 TÉMOIGNAGE / REPORTAGE — ANALYSE EN DIRECT".to_string(),
            video_source_type: "local".to_string(),
            video_file_path: String::new(),
            video_youtube_id: String::new(),
            video_anti_detect: true,
            video_volume: 85,
            video_cmd_id: 0,
            video_cmd_action: String::new(),
            video_yt_autoplay: true,
            video_yt_loop: false,
            video_yt_controls: true,
            video_yt_mute: false,
            preferred_stage_format: "9_16".to_string(),
            song_mode: "off".to_string(),
            song_requests: Vec::new(),
            current_song_requester: String::new(),
            song_vote: SongVoteState::default(),
            custom_backgrounds: CustomBackgrounds::default(),
            tapper_config: TapperConfig::default(),
            like_goal: LikeGoalConfig::default(),
            poll: PollState::default(),
            tiktok_host_account: "@lebyb".to_string(),
            tiktok_helper_account: "@second_compte".to_string(),
        }
    }
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn is_video_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| matches!(ext.to_lowercase().as_str(), "mp4" | "webm" | "mkv" | "mov" | "avi" | "m4v"))
        .unwrap_or(false)
}

pub fn extract_youtube_id(input: &str) -> String {
    let s = input.trim();
    if let Some(idx) = s.find("v=") {
        let rest = &s[idx + 2..];
        return rest.split(&['&', '?', '#', '/'][..]).next().unwrap_or("").to_string();
    }
    if let Some(idx) = s.find("youtu.be/") {
        let rest = &s[idx + 9..];
        return rest.split(&['&', '?', '#', '/'][..]).next().unwrap_or("").to_string();
    }
    if let Some(idx) = s.find("shorts/") {
        let rest = &s[idx + 7..];
        return rest.split(&['&', '?', '#', '/'][..]).next().unwrap_or("").to_string();
    }
    if let Some(idx) = s.find("embed/") {
        let rest = &s[idx + 6..];
        return rest.split(&['&', '?', '#', '/'][..]).next().unwrap_or("").to_string();
    }
    if !s.contains('/') && s.len() >= 8 && s.len() <= 20 {
        return s.to_string();
    }
    s.to_string()
}

/// Dossier racine du studio (celui qui contient les overlays HTML), détecté une seule fois :
/// dossier de l'exe, puis ses parents (cas de rust_studio/target/release), puis le dossier courant.
pub fn get_app_dir() -> std::path::PathBuf {
    static APP_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    APP_DIR
        .get_or_init(|| {
            if let Ok(exe) = std::env::current_exe() {
                for dir in exe.ancestors().skip(1).take(4) {
                    if dir.join("stage_solution_b.html").exists() {
                        return dir.to_path_buf();
                    }
                }
            }
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        })
        .clone()
}

/// Chemin d'un fichier relatif au dossier du studio.
fn app_path(rel: &str) -> PathBuf {
    get_app_dir().join(rel)
}

/// Dernier état sérialisé en attente d'écriture sur disque (None = rien de nouveau).
static PENDING_STATE_JSON: Mutex<Option<String>> = Mutex::new(None);

/// Sauvegarde différée : l'appelant ne fait que déposer le JSON, un thread d'arrière-plan
/// écrit au plus une fois par seconde et seulement si le contenu a changé.
fn save_state_json(st: &FullLiveState) {
    static WRITER: std::sync::Once = std::sync::Once::new();
    WRITER.call_once(|| {
        thread::spawn(|| {
            let mut last_written = String::new();
            loop {
                thread::sleep(Duration::from_millis(1000));
                let pending = PENDING_STATE_JSON.lock().ok().and_then(|mut p| p.take());
                if let Some(json_str) = pending {
                    if json_str != last_written {
                        let _ = std::fs::write(app_path("live_state.json"), &json_str);
                        let _ = std::fs::write(app_path(PREFERENCES_FILE), &json_str);
                        last_written = json_str;
                    }
                }
            }
        });
    });
    if let Ok(json_str) = serde_json::to_string_pretty(st) {
        if let Ok(mut p) = PENDING_STATE_JSON.lock() {
            *p = Some(json_str);
        }
    }
}

/// Écrit immédiatement la dernière sauvegarde en attente (à appeler à la fermeture).
fn flush_state_json() {
    let pending = PENDING_STATE_JSON.lock().ok().and_then(|mut p| p.take());
    if let Some(json_str) = pending {
        let _ = std::fs::write(app_path("live_state.json"), &json_str);
        let _ = std::fs::write(app_path(PREFERENCES_FILE), &json_str);
    }
}

fn save_emergency_file(list: &[EmergencyNumberItem]) {
    if let Ok(json_str) = serde_json::to_string_pretty(list) {
        let _ = std::fs::write(app_path(EMERGENCY_FILE), json_str);
    }
}

fn save_bot_config_file(cfg: &BotConfig) {
    if let Ok(json_str) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(app_path(BOT_CONFIG_FILE), json_str);
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
enum TelecommandeTab {
    Regie,
    Youtube,
    Articles,
    Sondages,
    Soundboard,
    Preferences,
    Chat,
    Bannieres,
}

struct TelecommandeApp {
    state: Arc<Mutex<FullLiveState>>,
    audio_peak_atomic: Arc<std::sync::atomic::AtomicU32>,
    library: SavedVideoLibrary,
    show_emergency_config_window: bool,
    show_bot_config_window: bool,
    new_emerg_number: String,
    new_emerg_label: String,
    new_trigger_kw: String,
    new_trigger_action: String,
    new_trigger_param: String,
    chat_only_questions: bool,
    active_tab: TelecommandeTab,
    mini_mode: bool,
    article_input_url: String,
    article_scrape_status: Arc<Mutex<String>>,
    video_input_yt_url: String,
    journal_status: String,
    chat_bridge: Option<std::process::Child>,
}

impl TelecommandeApp {
    fn new(state: Arc<Mutex<FullLiveState>>, audio_peak_atomic: Arc<std::sync::atomic::AtomicU32>) -> Self {
        let mut app = Self {
            state,
            audio_peak_atomic,
            library: SavedVideoLibrary {
                folder_path: app_path("videos").to_string_lossy().to_string(),
                files: Vec::new(),
            },
            show_emergency_config_window: false,
            show_bot_config_window: false,
            new_emerg_number: String::new(),
            new_emerg_label: String::new(),
            new_trigger_kw: String::new(),
            new_trigger_action: "emergency".to_string(),
            new_trigger_param: String::new(),
            chat_only_questions: false,
            active_tab: TelecommandeTab::Regie,
            mini_mode: false,
            article_input_url: String::new(),
            article_scrape_status: Arc::new(Mutex::new(String::new())),
            video_input_yt_url: String::new(),
            journal_status: String::new(),
            chat_bridge: None,
        };
        app.load_library();
        app.load_emergency_numbers();
        app.load_bot_config();
        app.load_jingles();
        app.load_preferences();
        app
    }

    fn load_preferences(&mut self) {
        let content_opt = std::fs::read_to_string(app_path(PREFERENCES_FILE)).ok()
            .or_else(|| std::fs::read_to_string(app_path("live_state.json")).ok());

        if let Some(content) = content_opt {
            if let Ok(saved_st) = serde_json::from_str::<FullLiveState>(&content) {
                if let Ok(mut st) = self.state.lock() {
                    st.branch = saved_st.branch;
                    st.custom_topic = saved_st.custom_topic;
                    st.custom_sub = saved_st.custom_sub;
                    st.tiktok_host_account = saved_st.tiktok_host_account;
                    st.tiktok_helper_account = saved_st.tiktok_helper_account;
                    st.chat_handle = saved_st.chat_handle;
                    st.custom_backgrounds = saved_st.custom_backgrounds;
                    st.like_goal = saved_st.like_goal;
                    st.poll = saved_st.poll;
                    st.tapper_config = saved_st.tapper_config;
                    st.article = saved_st.article;
                    st.video_yt_autoplay = saved_st.video_yt_autoplay;
                    st.video_yt_loop = saved_st.video_yt_loop;
                    st.video_yt_controls = saved_st.video_yt_controls;
                    st.video_yt_mute = saved_st.video_yt_mute;
                    st.video_anti_detect = saved_st.video_anti_detect;
                    st.emergency_visible = saved_st.emergency_visible;
                }
            }
        }
    }

    fn load_jingles(&mut self) {
        if let Ok(raw) = std::fs::read_to_string(app_path(JINGLES_FILE)) {
            if let Ok(saved) = serde_json::from_str::<Vec<JingleItem>>(&raw) {
                if !saved.is_empty() {
                    if let Ok(mut st) = self.state.lock() {
                        st.custom_jingles = saved;
                    }
                    return;
                }
            }
        }
        self.save_jingles();
    }

    fn save_jingles(&self) {
        if let Ok(st) = self.state.lock() {
            if let Ok(json_str) = serde_json::to_string_pretty(&st.custom_jingles) {
                let _ = std::fs::write(app_path(JINGLES_FILE),json_str);
            }
        }
    }

    fn trigger_jingle_item(&mut self, item_id: &str) {
        if let Ok(mut st) = self.state.lock() {
            if let Some(item) = st.custom_jingles.iter().find(|j| j.id == item_id).cloned() {
                st.jingle_cmd_id += 1;
                st.jingle_type = item.media_type;
                st.jingle_key = item.id;
                st.jingle_file_path = item.file_path;
                st.jingle_volume = item.volume;
                save_state_json(&st);
            }
        }
    }

    fn load_bot_config(&mut self) {
        if let Ok(raw) = std::fs::read_to_string(app_path(BOT_CONFIG_FILE)) {
            if let Ok(saved) = serde_json::from_str::<BotConfig>(&raw) {
                if let Ok(mut st) = self.state.lock() {
                    st.bot_config = saved;
                }
                return;
            }
        }
        if let Ok(st) = self.state.lock() {
            save_bot_config_file(&st.bot_config);
        }
    }

    fn start_tiktok_chat_bridge(&mut self) {
        let handle = {
            let mut st = self.state.lock().unwrap();
            st.chat_status = "⏳ Lancement du pont TikTok Live...".to_string();
            st.chat_handle.clone()
        };
        // Un seul pont à la fois : sinon chaque commentaire arriverait en double
        self.stop_tiktok_chat_bridge();
        let spawned = Command::new("python")
            .creation_flags(CREATE_NO_WINDOW)
            .current_dir(get_app_dir())
            .arg(app_path("tiktok_chat_bridge.py"))
            .arg(&handle)
            .spawn();
        match spawned {
            Ok(child) => self.chat_bridge = Some(child),
            Err(e) => {
                if let Ok(mut st) = self.state.lock() {
                    st.chat_status = format!("❌ Python introuvable ou erreur au lancement : {}", e);
                }
            }
        }
    }

    fn stop_tiktok_chat_bridge(&mut self) {
        if let Some(mut child) = self.chat_bridge.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn load_emergency_numbers(&mut self) {
        if let Ok(raw) = std::fs::read_to_string(app_path(EMERGENCY_FILE)) {
            if let Ok(saved) = serde_json::from_str::<Vec<EmergencyNumberItem>>(&raw) {
                if !saved.is_empty() {
                    if let Ok(mut st) = self.state.lock() {
                        st.emergency_numbers = saved;
                    }
                    return;
                }
            }
        }
        if let Ok(st) = self.state.lock() {
            save_emergency_file(&st.emergency_numbers);
        }
    }

    fn load_library(&mut self) {
        if let Ok(raw) = std::fs::read_to_string(app_path(LIBRARY_FILE)) {
            if let Ok(saved) = serde_json::from_str::<SavedVideoLibrary>(&raw) {
                self.library = saved;
            }
        }
        self.scan_library_folder();
    }

    fn save_library(&self) {
        if let Ok(json_str) = serde_json::to_string_pretty(&self.library) {
            let _ = std::fs::write(app_path(LIBRARY_FILE),json_str);
        }
    }

    fn scan_library_folder(&mut self) {
        let folder = PathBuf::from(&self.library.folder_path);
        if folder.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&folder) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && is_video_ext(&p) {
                        let s = p.to_string_lossy().to_string();
                        if !self.library.files.contains(&s) {
                            self.library.files.push(s);
                        }
                    }
                }
            }
        }
        self.library.files.retain(|f| Path::new(f).exists());
        self.save_library();
    }

    fn play_library_video(&mut self, full_path: &str) {
        let fname = Path::new(full_path)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Témoignage / Reportage");
        if let Ok(mut st) = self.state.lock() {
            st.video_file_path = full_path.to_string();
            st.video_source_type = "local".to_string();
            st.video_title = format!("🎥 {}", fname.replace('_', " "));
            st.video_active = true;
            st.branch_timestamp = now_secs();
            save_state_json(&st);
        }
    }

    fn set_branch(&mut self, branch_key: &str) {
        if let Ok(mut st) = self.state.lock() {
            apply_branch(&mut st, branch_key);
            save_state_json(&st);
        }
    }

    fn send_video_cmd(&mut self, action: &str) {
        if let Ok(mut st) = self.state.lock() {
            st.video_cmd_id += 1;
            st.video_cmd_action = action.to_string();
            save_state_json(&st);
        }
    }

    /// Ferme immédiatement toute fenêtre de scène live existante (STUDIO_LIVE_TIKTOK)
    fn close_all_stage_windows() {
        unsafe extern "system" fn enum_windows_close_stage(hwnd: windows::Win32::Foundation::HWND, _lparam: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::BOOL {
            use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextW, PostMessageW, WM_CLOSE};
            use windows::Win32::Foundation::{WPARAM, LPARAM, BOOL};
            let mut title_buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut title_buf);
            if len > 0 {
                let title = String::from_utf16_lossy(&title_buf[..len as usize]);
                if title.contains("STUDIO_LIVE_TIKTOK") {
                    let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            BOOL(1)
        }

        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
            use windows::Win32::Foundation::LPARAM;
            let _ = EnumWindows(Some(enum_windows_close_stage), LPARAM(0));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    fn open_stage_9_16_window() {
        // Fermer IMMÉDIATEMENT toute scène existante (9:16 ou 16:9) pour qu'il n'y en ait qu'une seule
        Self::close_all_stage_windows();

        let url = "http://127.0.0.1:8765/stage";
        let edge_paths = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ];
        for p in edge_paths {
            if std::path::Path::new(p).exists() {
                let _ = Command::new(p)
                    .arg(format!("--app={}", url))
                    .arg("--window-size=608,1080")
                    .arg("--window-position=20,0")
                    .arg("--disable-background-timer-throttling")
                    .arg("--disable-backgrounding-occluded-windows")
                    .arg("--disable-renderer-backgrounding")
                    .arg("--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling")
                    .spawn();
                return;
            }
        }
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }

    fn open_stage_16_9_window() {
        // Fermer IMMÉDIATEMENT toute scène existante (9:16 ou 16:9) pour qu'il n'y en ait qu'une seule
        Self::close_all_stage_windows();

        let url = "http://127.0.0.1:8765/stage_16_9";
        let edge_paths = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ];
        for p in edge_paths {
            if std::path::Path::new(p).exists() {
                let _ = Command::new(p)
                    .arg(format!("--app={}", url))
                    .arg("--start-maximized")
                    .arg("--window-size=1920,1080")
                    .arg("--window-position=0,0")
                    .arg("--disable-background-timer-throttling")
                    .arg("--disable-backgrounding-occluded-windows")
                    .arg("--disable-renderer-backgrounding")
                    .arg("--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling")
                    .spawn();
                return;
            }
        }
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }


    fn open_article_standalone_window() {
        let url = "http://127.0.0.1:8765/article";
        let edge_paths = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ];
        for p in edge_paths {
            if std::path::Path::new(p).exists() {
                let _ = Command::new(p)
                    .arg(format!("--app={}", url))
                    .arg("--window-size=680,780")
                    .arg("--window-position=100,80")
                    .arg("--disable-background-timer-throttling")
                    .arg("--disable-backgrounding-occluded-windows")
                    .arg("--disable-renderer-backgrounding")
                    .arg("--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling")
                    .spawn();
                return;
            }
        }
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }

    fn scrape_article_from_url(&mut self, url: &str) {
        let url = url.trim().to_string();
        if url.is_empty() {
            if let Ok(mut s) = self.article_scrape_status.lock() {
                *s = "⚠️ Veuillez saisir ou coller une URL valide.".to_string();
            }
            return;
        }

        if let Ok(mut s) = self.article_scrape_status.lock() {
            *s = "⏳ Extraction de l'article en cours...".to_string();
        }

        let state_clone = Arc::clone(&self.state);
        let status_clone = Arc::clone(&self.article_scrape_status);
        let url_clone = url.clone();

        thread::spawn(move || {
            let app_dir = get_app_dir();
            let scraper_path = app_dir.join("tools_python").join("scraper_article.py");
            let scraper_str = scraper_path.to_string_lossy().to_string();
            let out = Command::new("python")
                .arg(&scraper_str)
                .arg(&url_clone)
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            match out {
                Ok(output) => {
                    let raw_str = String::from_utf8_lossy(&output.stdout);
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw_str) {
                        if val.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                            let mut title_res = String::new();
                            if let Ok(mut st) = state_clone.lock() {
                                if let Some(title) = val.get("title").and_then(|v| v.as_str()) {
                                    if !title.is_empty() {
                                        st.article.title = title.to_string();
                                        title_res = title.to_string();
                                    }
                                }
                                if let Some(src) = val.get("source").and_then(|v| v.as_str()) {
                                    if !src.is_empty() {
                                        st.article.source = format!("📰 {}", src.to_uppercase());
                                    }
                                }
                                if let Some(date) = val.get("date").and_then(|v| v.as_str()) {
                                    st.article.date = date.to_string();
                                }
                                if let Some(txt) = val.get("text").and_then(|v| v.as_str()) {
                                    if !txt.is_empty() {
                                        st.article.text = txt.to_string();
                                    }
                                }
                                if let Some(img) = val.get("image_url").and_then(|v| v.as_str()) {
                                    st.article.image_url = img.to_string();
                                    st.article.show_image = !img.is_empty();
                                }
                                st.article.visible = true;
                                save_state_json(&st);
                            }
                            if let Ok(mut s) = status_clone.lock() {
                                *s = format!("✅ Article extrait avec succès : {}", title_res);
                            }
                        } else {
                            let err_msg = val.get("error").and_then(|v| v.as_str()).unwrap_or("Échec de l'extraction");
                            if let Ok(mut s) = status_clone.lock() {
                                *s = format!("❌ Erreur : {}", err_msg);
                            }
                        }
                    } else {
                        let err_details = String::from_utf8_lossy(&output.stderr);
                        if let Ok(mut s) = status_clone.lock() {
                            *s = format!("❌ Erreur analyse réponse : {}", err_details.chars().take(80).collect::<String>());
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut s) = status_clone.lock() {
                        *s = format!("❌ Erreur lancement python : {:?}", e);
                    }
                }
            }
        });
    }

    fn open_emergency_standalone_window() {
        let url = "http://127.0.0.1:8765/urgences";
        let edge_paths = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ];
        for p in edge_paths {
            if std::path::Path::new(p).exists() {
                let _ = Command::new(p)
                    .arg(format!("--app={}", url))
                    .arg("--window-size=520,320")
                    .arg("--window-position=80,120")
                    .arg("--disable-background-timer-throttling")
                    .arg("--disable-backgrounding-occluded-windows")
                    .arg("--disable-renderer-backgrounding")
                    .arg("--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling")
                    .spawn();
                return;
            }
        }
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }

    fn open_standalone_music_box() {
        if std::path::Path::new("Musique_Live_TikTok.exe").exists() {
            let _ = Command::new("Musique_Live_TikTok.exe").spawn();
        }
    }

    /// Fenêtre dédiée (Viewport OS indépendant ou modale) pour configurer facilement les numéros d'urgence
    fn render_emergency_config_viewport(&mut self, ctx: &egui::Context) {
        if !self.show_emergency_config_window {
            return;
        }

        let mut open = self.show_emergency_config_window;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("emergency_config_viewport"),
            egui::ViewportBuilder::default()
                .with_title("📞 CONFIGURATION DES NUMÉROS D'URGENCE & D'ÉCOUTE")
                .with_inner_size([620.0, 520.0])
                .with_always_on_top(),
            |ctx, _class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    open = false;
                }

                let panel_frame = egui::Frame::none()
                    .fill(Color32::from_rgb(18, 16, 26))
                    .inner_margin(egui::Margin::same(16.0));

                egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
                    ui.label(
                        RichText::new("📞 CONFIGURER LES NUMÉROS D'URGENCE & D'ÉCOUTE (VSS / INCESTE)")
                            .font(FontId::new(17.0, FontFamily::Proportional))
                            .strong()
                            .color(Color32::from_rgb(216, 180, 254)),
                    );
                    ui.label(
                        RichText::new("Coche ou décoche les numéros à afficher, modifie les textes en direct ou ajoute de nouveaux numéros.")
                            .size(12.0)
                            .color(Color32::from_rgb(209, 213, 219)),
                    );
                    ui.add_space(10.0);

                    let mut changed = false;
                    let mut to_delete: Option<usize> = None;

                    if let Ok(mut st) = self.state.lock() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Titre du bandeau d'urgence :").strong().color(Color32::WHITE));
                            if ui.text_edit_singleline(&mut st.emergency_title).changed() {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Style d'affichage sur le Live :").strong().color(Color32::WHITE));
                            if ui
                                .radio_value(
                                    &mut st.emergency_style,
                                    "grille".to_string(),
                                    "📋 Liste Complète Ultra-Lisible",
                                )
                                .changed()
                                || ui
                                    .radio_value(
                                        &mut st.emergency_style,
                                        "zoom".to_string(),
                                        "🔍 Liste + Zoom Géant Alterné (5s)",
                                    )
                                    .changed()
                            {
                                changed = true;
                            }

                            ui.add_space(10.0);
                            ui.label(RichText::new("Taille Texte :").strong().color(Color32::WHITE));
                            if ui
                                .add(egui::Slider::new(&mut st.emergency_scale_pct, 70..=160).suffix("%"))
                                .changed()
                            {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Format & Emplacement en 16:9 :").strong().color(Color32::WHITE));
                            if ui.selectable_value(&mut st.emergency_pos_16_9, "left".to_string(), "⬅️ Rectangle Gauche").clicked()
                                || ui.selectable_value(&mut st.emergency_pos_16_9, "right".to_string(), "➡️ Rectangle Droite").clicked()
                                || ui.selectable_value(&mut st.emergency_pos_16_9, "top".to_string(), "⬆️ Bandeau Haut").clicked()
                            {
                                changed = true;
                            }
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(6.0);

                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for (idx, item) in st.emergency_numbers.iter_mut().enumerate() {
                                    egui::Frame::none()
                                        .fill(Color32::from_rgb(28, 25, 40))
                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(88, 55, 135)))
                                        .rounding(Rounding::same(6.0))
                                        .inner_margin(egui::Margin::same(8.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                if ui.checkbox(&mut item.enabled, "").on_hover_text("Afficher sur le Live").changed() {
                                                    changed = true;
                                                }
                                                ui.label(RichText::new("N° :").strong().color(Color32::from_rgb(203, 213, 225)));
                                                if ui
                                                    .add(egui::TextEdit::singleline(&mut item.number).desired_width(115.0))
                                                    .changed()
                                                {
                                                    changed = true;
                                                }
                                                ui.label(RichText::new("Description :").strong().color(Color32::WHITE));
                                                if ui
                                                    .add(egui::TextEdit::singleline(&mut item.label).desired_width(270.0))
                                                    .changed()
                                                {
                                                    changed = true;
                                                }
                                                if ui.button("🗑️").on_hover_text("Supprimer cette ligne").clicked() {
                                                    to_delete = Some(idx);
                                                }
                                            });
                                        });
                                    ui.add_space(4.0);
                                }
                            });

                        if let Some(del_idx) = to_delete {
                            st.emergency_numbers.remove(del_idx);
                            changed = true;
                        }

                        ui.add_space(8.0);
                        // Ajouter un nouveau numéro
                        egui::Frame::none()
                            .fill(Color32::from_rgb(24, 34, 28))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(34, 197, 94)))
                            .rounding(Rounding::same(6.0))
                            .inner_margin(egui::Margin::same(8.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("➕ Nouveau N° :").strong().color(Color32::from_rgb(134, 239, 172)));
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_emerg_number)
                                            .hint_text("Ex: 17 / 112...")
                                            .desired_width(100.0),
                                    );
                                    ui.label("Texte :");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_emerg_label)
                                            .hint_text("Ex: Police / Secours d'urgence")
                                            .desired_width(220.0),
                                    );
                                    if ui.button(RichText::new("Ajouter").strong()).clicked()
                                        && !self.new_emerg_number.trim().is_empty()
                                    {
                                        st.emergency_numbers.push(EmergencyNumberItem {
                                            enabled: true,
                                            number: self.new_emerg_number.trim().to_string(),
                                            label: self.new_emerg_label.trim().to_string(),
                                        });
                                        self.new_emerg_number.clear();
                                        self.new_emerg_label.clear();
                                        changed = true;
                                    }
                                });
                            });

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("🔄 Remettre les numéros officiels par défaut").clicked() {
                                st.emergency_numbers = default_emergency_numbers();
                                changed = true;
                            }
                            if ui
                                .button(RichText::new("🪟 Ouvrir ces Numéros dans une Fenêtre Dédiée à part").strong().color(Color32::from_rgb(203, 213, 225)))
                                .clicked()
                            {
                                Self::open_emergency_standalone_window();
                            }
                            if ui.button("✅ Fermer").clicked() {
                                open = false;
                            }
                        });

                        if changed {
                            save_emergency_file(&st.emergency_numbers);
                            save_state_json(&st);
                        }
                    }
                });
            },
        );
        self.show_emergency_config_window = open;
    }

    /// Fenêtre dédiée pour configurer toutes les règles, mots-clés et réactions automatiques du Bot Live TikTok
    fn render_bot_config_viewport(&mut self, ctx: &egui::Context) {
        if !self.show_bot_config_window {
            return;
        }

        let mut open = self.show_bot_config_window;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("bot_config_viewport"),
            egui::ViewportBuilder::default()
                .with_title("🤖 CONFIGURATION DU BOT INTELLIGENT DU LIVE TIKTOK")
                .with_inner_size([650.0, 560.0])
                .with_always_on_top(),
            |ctx, _class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    open = false;
                }

                let panel_frame = egui::Frame::none()
                    .fill(Color32::from_rgb(18, 20, 28))
                    .inner_margin(egui::Margin::same(16.0));

                egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
                    ui.label(
                        RichText::new("🤖 CONFIGURATION DU BOT LIVE (Règles, Commandes & Mots-clés)")
                            .font(FontId::new(17.0, FontFamily::Proportional))
                            .strong()
                            .color(Color32::from_rgb(147, 197, 253)),
                    );
                    ui.label(
                        RichText::new("Configure comment le Bot réagit automatiquement aux messages du chat TikTok en direct.")
                            .size(12.0)
                            .color(Color32::from_rgb(203, 213, 225)),
                    );
                    ui.add_space(8.0);

                    let mut changed = false;
                    let mut to_delete: Option<usize> = None;

                    if let Ok(mut st) = self.state.lock() {
                        // Options Globales du Bot
                        egui::Frame::none()
                            .fill(Color32::from_rgb(26, 29, 40))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(59, 130, 246)))
                            .rounding(Rounding::same(6.0))
                            .inner_margin(egui::Margin::same(10.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("⚙️ COMPORTEMENTS GÉNÉRAUX").strong().color(Color32::WHITE));
                                ui.add_space(4.0);

                                if ui
                                    .checkbox(
                                        &mut st.bot_config.auto_emergency,
                                        "🚨 Afficher automatiquement le Panneau des Numéros d'Urgence dès qu'un mot de détresse est détecté",
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }

                                if ui
                                    .checkbox(
                                        &mut st.bot_config.auto_pin_questions,
                                        "❓ Épingler automatiquement toute question contenant '?' à l'écran Live",
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }

                                if ui
                                    .checkbox(
                                        &mut st.bot_config.filter_blacklist,
                                        "🛡️ Filtre Anti-Insultes (Ne jamais afficher les messages contenant des mots blacklistés)",
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }

                                ui.horizontal(|ui| {
                                    ui.label("Mots bannis (séparés par virgules) :");
                                    if ui
                                        .add(egui::TextEdit::singleline(&mut st.bot_config.blacklist_words).desired_width(320.0))
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });
                            });

                        ui.add_space(8.0);
                        ui.label(RichText::new("📋 RÈGLES & DÉCLENCHEURS DE COMMANDES :").strong().color(Color32::WHITE));

                        egui::ScrollArea::vertical()
                            .id_salt("bot_triggers_scroll")
                            .max_height(220.0)
                            .show(ui, |ui| {
                                for (idx, trig) in st.bot_config.triggers.iter_mut().enumerate() {
                                    egui::Frame::none()
                                        .fill(Color32::from_rgb(24, 27, 36))
                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(75, 85, 99)))
                                        .rounding(Rounding::same(6.0))
                                        .inner_margin(egui::Margin::same(8.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                if ui.checkbox(&mut trig.enabled, "").changed() {
                                                    changed = true;
                                                }
                                                ui.label(RichText::new("Mots :").strong().color(Color32::from_rgb(147, 197, 253)));
                                                if ui
                                                    .add(egui::TextEdit::singleline(&mut trig.keywords).desired_width(170.0))
                                                    .changed()
                                                {
                                                    changed = true;
                                                }

                                                ui.label("Action :");
                                                egui::ComboBox::from_id_salt(format!("trig_act_{}", idx))
                                                    .selected_text(match trig.action.as_str() {
                                                        "emergency" => "🚨 Afficher Urgences",
                                                        "jingle" => "🔊 Jouer Jingle",
                                                        "pin" => "📌 Épingler à l'écran",
                                                        _ => "Autre",
                                                    })
                                                    .show_ui(ui, |ui| {
                                                        if ui.selectable_value(&mut trig.action, "emergency".to_string(), "🚨 Afficher Urgences").clicked() {
                                                            changed = true;
                                                        }
                                                        if ui.selectable_value(&mut trig.action, "jingle".to_string(), "🔊 Jouer Jingle").clicked() {
                                                            changed = true;
                                                        }
                                                        if ui.selectable_value(&mut trig.action, "pin".to_string(), "📌 Épingler à l'écran").clicked() {
                                                            changed = true;
                                                        }
                                                    });

                                                if trig.action == "jingle" {
                                                    ui.label("Son :");
                                                    if ui.add(egui::TextEdit::singleline(&mut trig.action_param).desired_width(65.0)).changed() {
                                                        changed = true;
                                                    }
                                                }

                                                if ui.button("🗑️").on_hover_text("Supprimer cette règle").clicked() {
                                                    to_delete = Some(idx);
                                                }
                                            });
                                        });
                                    ui.add_space(4.0);
                                }
                            });

                        if let Some(del_idx) = to_delete {
                            st.bot_config.triggers.remove(del_idx);
                            changed = true;
                        }

                        ui.add_space(8.0);
                        // Ajouter une nouvelle règle
                        egui::Frame::none()
                            .fill(Color32::from_rgb(22, 28, 38))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(59, 130, 246)))
                            .rounding(Rounding::same(6.0))
                            .inner_margin(egui::Margin::same(8.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("➕ Nouvelle règle :").strong().color(Color32::from_rgb(147, 197, 253)));
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_trigger_kw)
                                            .hint_text("Ex: !merci, bravo")
                                            .desired_width(140.0),
                                    );
                                    egui::ComboBox::from_id_salt("new_trig_act_combo")
                                        .selected_text(match self.new_trigger_action.as_str() {
                                            "emergency" => "🚨 Urgences",
                                            "jingle" => "🔊 Jingle",
                                            _ => "📌 Épingler",
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut self.new_trigger_action, "emergency".to_string(), "🚨 Urgences");
                                            ui.selectable_value(&mut self.new_trigger_action, "jingle".to_string(), "🔊 Jingle");
                                            ui.selectable_value(&mut self.new_trigger_action, "pin".to_string(), "📌 Épingler");
                                        });

                                    if self.new_trigger_action == "jingle" {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.new_trigger_param)
                                                .hint_text("soutien / alerte...")
                                                .desired_width(90.0),
                                        );
                                    }

                                    if ui.button(RichText::new("Ajouter").strong()).clicked() && !self.new_trigger_kw.trim().is_empty() {
                                        st.bot_config.triggers.push(BotCustomTrigger {
                                            enabled: true,
                                            keywords: self.new_trigger_kw.trim().to_string(),
                                            action: self.new_trigger_action.clone(),
                                            action_param: self.new_trigger_param.trim().to_string(),
                                        });
                                        self.new_trigger_kw.clear();
                                        self.new_trigger_param.clear();
                                        changed = true;
                                    }
                                });
                            });

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("🔄 Rétablir les règles par défaut").clicked() {
                                st.bot_config = BotConfig::default();
                                changed = true;
                            }
                            if ui.button("✅ Fermer").clicked() {
                                open = false;
                            }
                        });

                        if changed {
                            save_bot_config_file(&st.bot_config);
                            save_state_json(&st);
                        }
                    }
                });
            },
        );
        self.show_bot_config_window = open;
    }

    fn toggle_shield(&mut self) {
        if let Ok(mut st) = self.state.lock() {
            st.shield_active = !st.shield_active;
            st.branch_timestamp = now_secs();
            save_state_json(&st);
        }
    }

    /// Bouton panique toujours visible (mode normal et mode mini) + message de pause modifiable
    fn render_shield_bar(&mut self, ui: &mut egui::Ui) {
        let (active, mut text) = {
            let st = self.state.lock().unwrap();
            (st.shield_active, st.shield_text.clone())
        };
        let (label, fill) = if active {
            ("🛡️ BOUCLIER ACTIF — Cliquer pour reprendre le live (F8)", Color32::from_rgb(185, 28, 28))
        } else {
            ("🛡️ BOUCLIER ANTI-RAID (F8)", Color32::from_rgb(60, 20, 20))
        };
        let btn = egui::Button::new(RichText::new(label).strong().size(11.5).color(Color32::WHITE))
            .fill(fill)
            .stroke(Stroke::new(1.5_f32, Color32::from_rgb(248, 113, 113)));
        if ui.add_sized([ui.available_width(), 30.0], btn).clicked() {
            self.toggle_shield();
        }
        if active {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Message :").size(10.5).color(Color32::from_rgb(252, 165, 165)));
                let edit = egui::TextEdit::singleline(&mut text)
                    .hint_text("⏸️ PAUSE MODÉRATION — Débat en cours de régulation")
                    .desired_width(ui.available_width());
                if ui.add(edit).changed() {
                    if let Ok(mut st) = self.state.lock() {
                        st.shield_text = text;
                        save_state_json(&st);
                    }
                }
            });
        }
        ui.add_space(4.0);
    }

    fn render_top_quick_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (current_branch, emerg_vis, tw_vis, vid_active, pref_fmt) = {
            let st = self.state.lock().unwrap();
            (
                st.branch.clone(),
                st.emergency_visible,
                st.trigger_warning_visible,
                st.video_active,
                st.preferred_stage_format.clone(),
            )
        };

        // Ligne 1: Titre compact + Switch Scène 9:16/16:9 + Bouton Mode Mini
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("🎛️ STUDIO LIVE")
                    .strong()
                    .size(13.0)
                    .color(Color32::WHITE),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Bouton Mode Mini
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("🗕 Mode Mini")
                                .strong()
                                .size(11.0)
                                .color(Color32::from_rgb(216, 180, 254)),
                        )
                        .fill(Color32::from_rgb(45, 30, 65)),
                    )
                    .clicked()
                {
                    self.mini_mode = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(310.0, 385.0)));
                }

                // Switch Format Scène 9:16 / 16:9
                if pref_fmt == "9_16" {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("📱 9:16 (➔ 🖥️ 16:9)")
                                    .size(10.5)
                                    .color(Color32::from_rgb(251, 146, 60)),
                            )
                            .fill(Color32::from_rgb(45, 35, 25)),
                        )
                        .clicked()
                    {
                        Self::open_stage_16_9_window();
                        if let Ok(mut st) = self.state.lock() {
                            st.preferred_stage_format = "16_9".to_string();
                            save_state_json(&st);
                        }
                    }
                } else {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("🖥️ 16:9 (➔ 📱 9:16)")
                                    .size(10.5)
                                    .color(Color32::from_rgb(52, 211, 153)),
                            )
                            .fill(Color32::from_rgb(25, 45, 35)),
                        )
                        .clicked()
                    {
                        Self::open_stage_9_16_window();
                        if let Ok(mut st) = self.state.lock() {
                            st.preferred_stage_format = "9_16".to_string();
                            save_state_json(&st);
                        }
                    }
                }
            });
        });

        ui.add_space(3.0);

        // Ligne 2: Les 4 Branches Principales (4 boutons calculés dynamiquement sans jamais déborder)
        ui.horizontal(|ui| {
            let total_w = ui.available_width();
            let btn_w = (total_w - 15.0) / 4.0;

            let b1_col = if current_branch == "vss" { Color32::from_rgb(147, 51, 234) } else { Color32::from_rgb(38, 25, 52) };
            if ui.add_sized([btn_w, 30.0], egui::Button::new(RichText::new("💜 [F9] VSS").strong().size(11.0).color(Color32::WHITE)).fill(b1_col)).clicked() {
                self.set_branch("vss");
            }

            let b2_col = if current_branch == "politique" { Color32::from_rgb(220, 38, 38) } else { Color32::from_rgb(52, 25, 25) };
            if ui.add_sized([btn_w, 30.0], egui::Button::new(RichText::new("[F10] Politique").strong().size(11.0).color(Color32::WHITE)).fill(b2_col)).clicked() {
                self.set_branch("politique");
            }

            let b3_col = if current_branch == "musique" { Color32::from_rgb(217, 119, 6) } else { Color32::from_rgb(52, 35, 18) };
            if ui.add_sized([btn_w, 30.0], egui::Button::new(RichText::new("[F11] Musique").strong().size(11.0).color(Color32::WHITE)).fill(b3_col)).clicked() {
                self.set_branch("musique");
            }

            let b4_col = if current_branch == "chill" { Color32::from_rgb(16, 185, 129) } else { Color32::from_rgb(20, 48, 38) };
            if ui.add_sized([btn_w, 30.0], egui::Button::new(RichText::new("☕ Chill").strong().size(11.0).color(Color32::WHITE)).fill(b4_col)).clicked() {
                self.set_branch("chill");
            }
        });

        ui.add_space(3.0);

        // Ligne 3: Toggles d'Actions Rapides (Urgences, TW, Vidéo, Musique Box)
        ui.horizontal(|ui| {
            let total_w = ui.available_width();
            let btn_w = (total_w - 12.0) / 4.0;

            // 🚨 Urgences
            let (emerg_txt, emerg_bg) = if emerg_vis {
                ("[!] Urg. ON", Color32::from_rgb(220, 38, 38))
            } else {
                ("[!] Urg. OFF", Color32::from_rgb(38, 42, 54))
            };
            if ui
                .add_sized(
                    [btn_w, 24.0],
                    egui::Button::new(
                        RichText::new(emerg_txt)
                            .strong()
                            .size(10.5)
                            .color(Color32::WHITE),
                    )
                    .fill(emerg_bg),
                )
                .clicked()
            {
                if let Ok(mut st) = self.state.lock() {
                    st.emergency_visible = !st.emergency_visible;
                    st.branch_timestamp = now_secs();
                    save_state_json(&st);
                }
            }

            // ⚠️ TW
            let (tw_txt, tw_bg) = if tw_vis {
                ("[TW] ON", Color32::from_rgb(217, 119, 6))
            } else {
                ("[TW] OFF", Color32::from_rgb(38, 42, 54))
            };
            if ui
                .add_sized(
                    [btn_w, 24.0],
                    egui::Button::new(
                        RichText::new(tw_txt)
                            .strong()
                            .size(10.5)
                            .color(Color32::WHITE),
                    )
                    .fill(tw_bg),
                )
                .clicked()
            {
                if let Ok(mut st) = self.state.lock() {
                    st.trigger_warning_visible = !st.trigger_warning_visible;
                    save_state_json(&st);
                }
            }

            // 🎬 Vidéo
            let (vid_txt, vid_bg) = if vid_active {
                ("[VID] ON", Color32::from_rgb(37, 99, 235))
            } else {
                ("[VID] OFF", Color32::from_rgb(38, 42, 54))
            };
            if ui
                .add_sized(
                    [btn_w, 24.0],
                    egui::Button::new(
                        RichText::new(vid_txt)
                            .strong()
                            .size(10.5)
                            .color(Color32::WHITE),
                    )
                    .fill(vid_bg),
                )
                .clicked()
            {
                if let Ok(mut st) = self.state.lock() {
                    st.video_active = !st.video_active;
                    st.branch_timestamp = now_secs();
                    save_state_json(&st);
                }
            }

            // 🎵 Musique Box
            if ui
                .add_sized(
                    [btn_w, 24.0],
                    egui::Button::new(
                        RichText::new("🎵 Jukebox")
                            .size(10.5)
                            .color(Color32::from_rgb(203, 213, 225)),
                    )
                    .fill(Color32::from_rgb(38, 42, 54)),
                )
                .clicked()
            {
                Self::open_standalone_music_box();
            }
        });
    }

    fn render_tab_selector(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            let total_w = ui.available_width();
            let tab_w = (total_w - 10.0) / 3.0;

            let row1 = [
                (TelecommandeTab::Regie, "[ Régie ]", Color32::from_rgb(147, 51, 234)),
                (TelecommandeTab::Youtube, "[ YouTube ]", Color32::from_rgb(239, 68, 68)),
                (TelecommandeTab::Articles, "[ Presse ]", Color32::from_rgb(234, 88, 12)),
            ];

            let row2 = [
                (TelecommandeTab::Sondages, "🗳️ Sondages", Color32::from_rgb(59, 130, 246)),
                (TelecommandeTab::Soundboard, "🎵 Sons", Color32::from_rgb(16, 185, 129)),
                (TelecommandeTab::Preferences, "[ Préférences ]", Color32::from_rgb(107, 114, 128)),
            ];

            ui.horizontal(|ui| {
                for (tab, label, active_col) in row1 {
                    let is_sel = self.active_tab == tab;
                    let bg = if is_sel { active_col } else { Color32::from_rgb(28, 30, 38) };
                    let txt_col = if is_sel { Color32::WHITE } else { Color32::from_rgb(156, 163, 175) };
                    if ui.add_sized([tab_w, 28.0], egui::Button::new(RichText::new(label).strong().size(12.0).color(txt_col)).fill(bg)).clicked() {
                        self.active_tab = tab;
                    }
                }
            });

            ui.add_space(2.0);

            ui.horizontal(|ui| {
                for (tab, label, active_col) in row2 {
                    let is_sel = self.active_tab == tab;
                    let bg = if is_sel { active_col } else { Color32::from_rgb(28, 30, 38) };
                    let txt_col = if is_sel { Color32::WHITE } else { Color32::from_rgb(156, 163, 175) };
                    if ui.add_sized([tab_w, 28.0], egui::Button::new(RichText::new(label).strong().size(12.0).color(txt_col)).fill(bg)).clicked() {
                        self.active_tab = tab;
                    }
                }
            });
        });
    }

    fn render_mini_dashboard(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (current_branch, emerg_vis, tw_vis, vid_active, track, pinned_vis, pinned_author, pinned_txt, custom_topic) = {
            let st = self.state.lock().unwrap();
            (
                st.branch.clone(),
                st.emergency_visible,
                st.trigger_warning_visible,
                st.video_active,
                st.spotify.clone(),
                st.pinned_visible,
                st.pinned_author.clone(),
                st.pinned_text.clone(),
                st.custom_topic.clone(),
            )
        };

        // Header Mini Mode
        ui.horizontal(|ui| {
            ui.label(RichText::new("🎛️ MINI-RÉGIE").strong().size(13.0).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("↗ Déplier")
                                .strong()
                                .size(11.0)
                                .color(Color32::from_rgb(216, 180, 254)),
                        )
                        .fill(Color32::from_rgb(58, 35, 80)),
                    )
                    .clicked()
                {
                    self.mini_mode = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(460.0, 580.0)));
                }
            });
        });

        ui.add_space(4.0);

        // 3 Branches
        ui.horizontal(|ui| {
            let btn_w = (ui.available_width() - 8.0) / 3.0;
            let b1 = egui::Button::new(RichText::new("💜 VSS").strong().size(11.5).color(Color32::WHITE))
                .fill(if current_branch == "vss" { Color32::from_rgb(147, 51, 234) } else { Color32::from_rgb(38, 25, 52) });
            if ui.add_sized([btn_w, 30.0], b1).clicked() { self.set_branch("vss"); }

            let b2 = egui::Button::new(RichText::new("✊ Pol.").strong().size(11.5).color(Color32::WHITE))
                .fill(if current_branch == "politique" { Color32::from_rgb(220, 38, 38) } else { Color32::from_rgb(52, 25, 25) });
            if ui.add_sized([btn_w, 30.0], b2).clicked() { self.set_branch("politique"); }

            let b3 = egui::Button::new(RichText::new("🎸 Mus.").strong().size(11.5).color(Color32::WHITE))
                .fill(if current_branch == "musique" { Color32::from_rgb(217, 119, 6) } else { Color32::from_rgb(52, 35, 18) });
            if ui.add_sized([btn_w, 30.0], b3).clicked() { self.set_branch("musique"); }
        });

        ui.add_space(4.0);

        // 3 Toggles
        ui.horizontal(|ui| {
            let btn_w = (ui.available_width() - 8.0) / 3.0;
            let emerg_btn = egui::Button::new(RichText::new(if emerg_vis { "🚨 URG. ON" } else { "[!] Urg. OFF" }).strong().size(10.5).color(Color32::WHITE))
                .fill(if emerg_vis { Color32::from_rgb(220, 38, 38) } else { Color32::from_rgb(38, 42, 54) });
            if ui.add_sized([btn_w, 26.0], emerg_btn).clicked() {
                if let Ok(mut st) = self.state.lock() {
                    st.emergency_visible = !st.emergency_visible;
                    st.branch_timestamp = now_secs();
                    save_state_json(&st);
                }
            }

            let tw_btn = egui::Button::new(RichText::new(if tw_vis { "[TW] ON" } else { "[TW] OFF" }).strong().size(10.5).color(Color32::WHITE))
                .fill(if tw_vis { Color32::from_rgb(217, 119, 6) } else { Color32::from_rgb(38, 42, 54) });
            if ui.add_sized([btn_w, 26.0], tw_btn).clicked() {
                if let Ok(mut st) = self.state.lock() {
                    st.trigger_warning_visible = !st.trigger_warning_visible;
                    save_state_json(&st);
                }
            }

            let vid_btn = egui::Button::new(RichText::new(if vid_active { "🎬 VID. ON" } else { "[VID] OFF" }).strong().size(10.5).color(Color32::WHITE))
                .fill(if vid_active { Color32::from_rgb(37, 99, 235) } else { Color32::from_rgb(38, 42, 54) });
            if ui.add_sized([btn_w, 26.0], vid_btn).clicked() {
                if let Ok(mut st) = self.state.lock() {
                    st.video_active = !st.video_active;
                    st.branch_timestamp = now_secs();
                    save_state_json(&st);
                }
            }
        });

        ui.add_space(4.0);

        // Sujet en direct
        ui.horizontal(|ui| {
            ui.label(RichText::new("📢").size(11.0));
            ui.add(egui::Label::new(RichText::new(if custom_topic.is_empty() { "Sujet non défini" } else { &custom_topic }).size(11.0).color(Color32::from_rgb(203, 213, 225))).truncate());
        });

        // Musique Spotify en direct
        ui.horizontal(|ui| {
            let icon = if track.playing { "▶️" } else { "⏸️" };
            ui.add(egui::Label::new(RichText::new(format!("{} {}", icon, if track.title.is_empty() { "Spotify inactif" } else { &track.title })).size(11.0).color(Color32::from_rgb(147, 197, 253))).truncate());
            if track.connected {
                if ui.small_button("⏯️").clicked() { trigger_media_play_pause(); }
            }
        });

        // Niveau Sonore PC (VU-Mètre compact)
        let peak = f32::from_bits(self.audio_peak_atomic.load(std::sync::atomic::Ordering::Relaxed));
        ui.horizontal(|ui| {
            ui.label(RichText::new("🔊").size(10.5));
            let meter_color = if peak > 0.85 {
                Color32::from_rgb(239, 68, 68)
            } else if peak > 0.6 {
                Color32::from_rgb(245, 158, 11)
            } else {
                Color32::from_rgb(34, 197, 94)
            };
            ui.add(egui::ProgressBar::new(peak).desired_width(170.0).fill(meter_color));
            ui.label(RichText::new(format!("{:.0}%", peak * 100.0)).size(10.0).color(Color32::from_rgb(148, 163, 184)));
        });

        // Message épinglé en direct
        if pinned_vis {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(format!("📌 {}: {}", pinned_author, pinned_txt)).size(10.5).color(Color32::from_rgb(250, 204, 21))).truncate());
                if ui.small_button("❌").clicked() {
                    if let Ok(mut st) = self.state.lock() {
                        st.pinned_visible = false;
                        save_state_json(&st);
                    }
                }
            });
        }

        ui.add_space(6.0);
        // Quick Jingles
        ui.horizontal(|ui| {
            let btn_w = (ui.available_width() - 4.0) / 2.0;
            if ui.add_sized([btn_w, 26.0], egui::Button::new(RichText::new("🚨 Sirène").size(10.5).color(Color32::WHITE)).fill(Color32::from_rgb(185, 28, 28))).clicked() {
                self.trigger_jingle_item("sirene");
            }
            if ui.add_sized([btn_w, 26.0], egui::Button::new(RichText::new("📢 Airhorn").size(10.5).color(Color32::WHITE)).fill(Color32::from_rgb(180, 83, 9))).clicked() {
                self.trigger_jingle_item("airhorn");
            }
        });
    }

    fn timer_start(&mut self, secs: u32) {
        if let Ok(mut st) = self.state.lock() {
            st.timer_active = true;
            st.timer_running = true;
            st.timer_total_secs = secs;
            st.timer_end_ts = now_secs() + secs as f64;
            st.timer_remaining_secs = secs as f64;
            st.timer_gong_done = false;
            save_state_json(&st);
        }
    }

    fn timer_add(&mut self, secs: f64) {
        if let Ok(mut st) = self.state.lock() {
            if !st.timer_active {
                return;
            }
            if st.timer_running {
                st.timer_end_ts = st.timer_end_ts.max(now_secs()) + secs;
            } else {
                st.timer_remaining_secs = st.timer_remaining_secs.max(0.0) + secs;
            }
            st.timer_total_secs += secs as u32;
            st.timer_gong_done = false;
            save_state_json(&st);
        }
    }

    fn timer_pause_toggle(&mut self) {
        if let Ok(mut st) = self.state.lock() {
            if !st.timer_active {
                return;
            }
            if st.timer_running {
                st.timer_remaining_secs = (st.timer_end_ts - now_secs()).max(0.0);
                st.timer_running = false;
            } else {
                st.timer_end_ts = now_secs() + st.timer_remaining_secs;
                st.timer_running = true;
            }
            save_state_json(&st);
        }
    }

    fn timer_stop(&mut self) {
        if let Ok(mut st) = self.state.lock() {
            st.timer_active = false;
            st.timer_running = false;
            save_state_json(&st);
        }
    }

    /// Chrono de débat : préréglages 1/3/5 min, +30 s, pause et arrêt, avec le temps restant en direct
    fn render_debate_timer(&mut self, ui: &mut egui::Ui) {
        let (active, running, mut label, remaining) = {
            let st = self.state.lock().unwrap();
            let rem = if st.timer_running { st.timer_end_ts - now_secs() } else { st.timer_remaining_secs };
            (st.timer_active, st.timer_running, st.timer_label.clone(), rem.max(0.0))
        };
        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(20, 22, 30))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(234, 179, 8)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⏱️ CHRONO DE DÉBAT").strong().color(Color32::from_rgb(250, 204, 21)));
                    if active {
                        let secs = remaining.ceil() as u32;
                        let col = if secs <= 15 {
                            Color32::from_rgb(239, 68, 68)
                        } else if secs <= 30 {
                            Color32::from_rgb(249, 115, 22)
                        } else {
                            Color32::WHITE
                        };
                        let txt = if secs == 0 { "⏰ TEMPS ÉCOULÉ".to_string() } else { format!("{:02}:{:02}", secs / 60, secs % 60) };
                        ui.label(RichText::new(txt).strong().size(16.0).color(col));
                        if !running {
                            ui.label(RichText::new("(pause)").size(10.5).color(Color32::GRAY));
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Intervenant·e :").size(10.5));
                    let edit = egui::TextEdit::singleline(&mut label)
                        .hint_text("ex : @pseudo, Invité·e, Question du chat…")
                        .desired_width(ui.available_width());
                    if ui.add(edit).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.timer_label = label;
                            save_state_json(&st);
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (txt, secs) in [("1 min", 60), ("3 min", 180), ("5 min", 300)] {
                        if ui.button(RichText::new(format!("▶ {}", txt)).strong()).clicked() {
                            self.timer_start(secs);
                        }
                    }
                    if active {
                        if ui.button("+30 s").clicked() {
                            self.timer_add(30.0);
                        }
                        if ui.button(if running { "⏸ Pause" } else { "▶ Reprendre" }).clicked() {
                            self.timer_pause_toggle();
                        }
                        if ui.button("⏹ Stop").clicked() {
                            self.timer_stop();
                        }
                    }
                });
            });
        if active && running {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }

    /// Journal du live : résumé en cours + export Markdown dans journaux/ (« Terminer le live »)
    fn render_journal_controls(&mut self, ui: &mut egui::Ui) {
        let (start, comments, events) = self
            .state
            .lock()
            .map(|st| (format!("{} {}", st.journal.start_date, st.journal.start_time), st.journal.comments, st.journal.events.len()))
            .unwrap_or_default();
        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(20, 22, 30))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(56, 189, 248)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.label(RichText::new("📝 JOURNAL DU LIVE").strong().color(Color32::from_rgb(125, 211, 252)));
                ui.label(
                    RichText::new(format!("Depuis {} • {} commentaires • {} événements notés", start, comments, events))
                        .size(10.5)
                        .color(Color32::GRAY),
                );
                ui.horizontal(|ui| {
                    let btn = egui::Button::new(RichText::new("🏁 Terminer le live & enregistrer le journal").strong().color(Color32::WHITE))
                        .fill(Color32::from_rgb(3, 105, 161));
                    if ui.add(btn).clicked() {
                        let result = self.state.lock().map_err(|e| e.to_string()).and_then(|mut st| {
                            let r = export_journal(&mut st);
                            save_state_json(&st);
                            r
                        });
                        self.journal_status = match result {
                            Ok(path) => format!("✅ Enregistré : {}", path.display()),
                            Err(e) => format!("❌ Échec de l'enregistrement : {}", e),
                        };
                    }
                    if ui.button("📂 Dossier").on_hover_text("Ouvrir le dossier des journaux").clicked() {
                        let dir = app_path("journaux");
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = Command::new("explorer").arg(&dir).spawn();
                    }
                });
                if !self.journal_status.is_empty() {
                    ui.label(RichText::new(&self.journal_status).size(10.5));
                }
            });
    }

    /// File privée des demandes d'aide : pseudo, heure, message, copie du pseudo et marquage « traité »
    fn render_help_alerts(&mut self, ui: &mut egui::Ui) {
        let alerts = self.state.lock().map(|st| st.help_alerts.clone()).unwrap_or_default();
        let pending = alerts.iter().filter(|a| !a.handled).count();
        let title = format!("🆘 DEMANDES D'AIDE — {} à traiter / {} au total", pending, alerts.len());
        let title_col = if pending > 0 { Color32::from_rgb(248, 113, 113) } else { Color32::from_rgb(134, 239, 172) };
        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(20, 22, 30))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(239, 68, 68)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                egui::CollapsingHeader::new(RichText::new(title).strong().color(title_col))
                    .id_salt("help_alerts_panel")
                    .default_open(pending > 0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Privé : jamais affiché sur les scènes. Pour recontacter les personnes après le live.")
                                .size(10.0)
                                .color(Color32::GRAY),
                        );
                        if alerts.is_empty() {
                            ui.label(RichText::new("Aucune demande pour l'instant.").italics().size(10.5));
                            return;
                        }
                        let mut toggle_id: Option<u64> = None;
                        egui::ScrollArea::vertical().max_height(220.0).id_salt("help_alerts_scroll").show(ui, |ui| {
                            for a in alerts.iter().rev() {
                                ui.horizontal_wrapped(|ui| {
                                    let name_col = if a.handled { Color32::GRAY } else { Color32::WHITE };
                                    ui.label(RichText::new(&a.time_label).size(10.0).color(Color32::GRAY));
                                    ui.label(RichText::new(format!("{} {}", a.author, a.handle)).strong().color(name_col));
                                    if ui.small_button("📋").on_hover_text("Copier le pseudo").clicked() {
                                        let who = if a.handle.is_empty() { a.author.clone() } else { a.handle.clone() };
                                        ui.ctx().copy_text(who);
                                    }
                                    let lbl = if a.handled { "↩ À traiter" } else { "✅ Traité" };
                                    if ui.small_button(lbl).clicked() {
                                        toggle_id = Some(a.id);
                                    }
                                });
                                let txt_col = if a.handled { Color32::DARK_GRAY } else { Color32::from_rgb(254, 202, 202) };
                                ui.label(RichText::new(&a.text).size(11.0).color(txt_col));
                                ui.separator();
                            }
                        });
                        let mut clear_handled = false;
                        if ui.button("🗑 Effacer les demandes traitées").clicked() {
                            clear_handled = true;
                        }
                        if toggle_id.is_some() || clear_handled {
                            if let Ok(mut st) = self.state.lock() {
                                if let Some(id) = toggle_id {
                                    if let Some(a) = st.help_alerts.iter_mut().find(|a| a.id == id) {
                                        a.handled = !a.handled;
                                    }
                                }
                                if clear_handled {
                                    st.help_alerts.retain(|a| !a.handled);
                                }
                                save_state_json(&st);
                            }
                        }
                    });
            });
    }

    /// Remerciements auto (cadeaux, partages, follows) : activation + bouton d'aperçu sur les scènes
    fn render_thanks_controls(&mut self, ui: &mut egui::Ui) {
        let mut enabled = self.state.lock().map(|st| st.thanks_enabled).unwrap_or(true);
        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(20, 22, 30))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(192, 132, 252)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("💜 REMERCIEMENTS AUTO").strong().color(Color32::from_rgb(216, 180, 254)));
                    if ui.checkbox(&mut enabled, "Cadeaux, partages & follows").changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.thanks_enabled = enabled;
                            save_state_json(&st);
                        }
                    }
                    if ui.button("👁 Aperçu").clicked() {
                        if let Ok(mut st) = self.state.lock() {
                            push_thanks(&mut st, "gift", "Spectateur test".to_string(), "Rose".to_string(), 3);
                            save_state_json(&st);
                        }
                    }
                });
            });
    }

    fn render_tab_regie(&mut self, ui: &mut egui::Ui) {
                self.render_debate_timer(ui);
                ui.add_space(8.0);
                self.render_thanks_controls(ui);
                ui.add_space(8.0);
                self.render_help_alerts(ui);
                ui.add_space(8.0);
                self.render_journal_controls(ui);
                ui.add_space(8.0);

                // =========================================================
                // 1. SÉLECTION RAPIDE DE BRANCHE (VSS / POLITIQUE / MUSIQUE)
                // =========================================================
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(20, 22, 30))
                    .stroke(Stroke::new(1.2_f32, Color32::from_rgb(59, 130, 246)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        let cur_br = {
                            let st = self.state.lock().unwrap();
                            st.branch.clone()
                        };
                        ui.label(RichText::new("🎛️ 1. CHOIX DE LA BRANCHE DU LIVE").strong().color(Color32::from_rgb(96, 165, 250)));
                        ui.add_space(6.0);

                        ui.vertical(|ui| {
                            let w = (ui.available_width() - 6.0) / 2.0;

                            ui.horizontal(|ui| {
                                let b1 = egui::Button::new(RichText::new("💜 VSS & INCESTE").strong().size(11.5).color(Color32::WHITE))
                                    .fill(if cur_br == "vss" { Color32::from_rgb(147, 51, 234) } else { Color32::from_rgb(38, 25, 52) })
                                    .stroke(if cur_br == "vss" { Stroke::new(2.0_f32, Color32::from_rgb(216, 180, 254)) } else { Stroke::NONE });
                                if ui.add_sized([w, 32.0], b1).clicked() { self.set_branch("vss"); }

                                let b2 = egui::Button::new(RichText::new("✊ POLITISER (Antifa)").strong().size(11.5).color(Color32::WHITE))
                                    .fill(if cur_br == "politique" { Color32::from_rgb(220, 38, 38) } else { Color32::from_rgb(52, 25, 25) })
                                    .stroke(if cur_br == "politique" { Stroke::new(2.0_f32, Color32::from_rgb(252, 165, 165)) } else { Stroke::NONE });
                                if ui.add_sized([w, 32.0], b2).clicked() { self.set_branch("politique"); }
                            });

                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                let b3 = egui::Button::new(RichText::new("🎸 MUSIQUE LIVE").strong().size(11.5).color(Color32::WHITE))
                                    .fill(if cur_br == "musique" { Color32::from_rgb(217, 119, 6) } else { Color32::from_rgb(52, 35, 18) })
                                    .stroke(if cur_br == "musique" { Stroke::new(2.0_f32, Color32::from_rgb(253, 186, 116)) } else { Stroke::NONE });
                                if ui.add_sized([w, 32.0], b3).clicked() { self.set_branch("musique"); }

                                let b4 = egui::Button::new(RichText::new("☕ CHILL & DÉTENTE").strong().size(11.5).color(Color32::WHITE))
                                    .fill(if cur_br == "chill" { Color32::from_rgb(16, 185, 129) } else { Color32::from_rgb(20, 48, 38) })
                                    .stroke(if cur_br == "chill" { Stroke::new(2.0_f32, Color32::from_rgb(110, 231, 183)) } else { Stroke::NONE });
                                if ui.add_sized([w, 32.0], b4).clicked() { self.set_branch("chill"); }
                            });
                        });
                    });

                ui.add_space(8.0);
                // =========================================================
                // 4. TEXTE DU SUJET & MODE DU BANDEAU MUSIQUE
                // =========================================================
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(24, 26, 34))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(55, 60, 75)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        let (mut topic, mut sub, mut mode) = {
                            let st = self.state.lock().unwrap();
                            (st.custom_topic.clone(), st.custom_sub.clone(), st.box_mode.clone())
                        };

                        ui.label(
                            RichText::new("✍️ 4. CONTENU DU LECTEUR & BANDEAU")
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.horizontal(|ui| {
                            if ui.radio_value(&mut mode, "combine".to_string(), "🔄 Sujet + Musique").changed()
                                || ui.radio_value(&mut mode, "musique".to_string(), "🎵 Musique seule").changed()
                                || ui.radio_value(&mut mode, "sujet".to_string(), "📢 Sujet seul").changed()
                            {
                                if let Ok(mut st) = self.state.lock() {
                                    st.box_mode = mode.clone();
                                    save_state_json(&st);
                                }
                            }
                        });

                        ui.label("Titre / Sujet du Live :");
                        if ui.text_edit_singleline(&mut topic).changed() {
                            if let Ok(mut st) = self.state.lock() {
                                st.custom_topic = topic;
                                save_state_json(&st);
                            }
                        }

                        ui.label("Sous-texte :");
                        if ui.text_edit_singleline(&mut sub).changed() {
                            if let Ok(mut st) = self.state.lock() {
                                st.custom_sub = sub;
                                save_state_json(&st);
                            }
                        }

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Taille Sujet :").size(11.0).color(Color32::from_rgb(203, 213, 225)));
                            let (mut t_sz, mut s_sz) = {
                                let st = self.state.lock().unwrap();
                                (st.custom_topic_font_size, st.custom_sub_font_size)
                            };
                            if ui.add(egui::Slider::new(&mut t_sz, 12..=54).suffix(" px")).changed() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_topic_font_size = t_sz;
                                    save_state_json(&st);
                                }
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Taille Sous-texte :").size(11.0).color(Color32::from_rgb(203, 213, 225)));
                            if ui.add(egui::Slider::new(&mut s_sz, 10..=36).suffix(" px")).changed() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_sub_font_size = s_sz;
                                    save_state_json(&st);
                                }
                            }
                        });
                    });

                ui.add_space(8.0);
                // =========================================================
                // 📢 ENCART ANNONCE DU LIVE & TEXTE LIBRE
                // =========================================================
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(22, 25, 36))
                    .stroke(Stroke::new(1.2_f32, Color32::from_rgb(168, 85, 247)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        let (
                            mut box_vis,
                            mut has_title,
                            mut title,
                            mut title_font_sz,
                            mut text,
                            mut font_sz,
                            mut align,
                            mut bg_opac,
                            mut border_w,
                            mut border_rad,
                            mut padding,
                        ) = {
                            let st = self.state.lock().unwrap();
                            (
                                st.custom_box_visible,
                                st.custom_box_has_title,
                                st.custom_box_title.clone(),
                                st.custom_box_title_font_size,
                                st.custom_box_text.clone(),
                                st.custom_box_font_size,
                                st.custom_box_align.clone(),
                                st.custom_box_bg_opacity,
                                st.custom_box_border_width,
                                st.custom_box_border_radius,
                                st.custom_box_padding,
                            )
                        };

                        let mut changed = false;

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("[4] ANNONCE DU LIVE (ENCART TEXTE LIBRE)").strong().color(Color32::from_rgb(216, 180, 254)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (btn_txt, btn_col) = if box_vis {
                                    ("⏹️ MASQUER L'ANNONCE", Color32::from_rgb(220, 38, 38))
                                } else {
                                    ("▶️ AFFICHER SUR LE LIVE", Color32::from_rgb(34, 197, 94))
                                };
                                if ui.add(egui::Button::new(RichText::new(btn_txt).strong().color(Color32::WHITE)).fill(btn_col)).clicked() {
                                    box_vis = !box_vis;
                                    changed = true;
                                }
                            });
                        });

                        ui.add_space(6.0);

                        // Option Titre
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut has_title, RichText::new("🏷️ Activer un Titre").strong().color(Color32::from_rgb(147, 197, 253))).changed() {
                                changed = true;
                            }
                            if has_title {
                                ui.label(RichText::new("Titre :").color(Color32::WHITE));
                                if ui.add(egui::TextEdit::singleline(&mut title).desired_width(180.0)).changed() {
                                    changed = true;
                                }
                                ui.label(RichText::new("Taille titre :").size(11.0).color(Color32::from_rgb(203, 213, 225)));
                                if ui.add(egui::Slider::new(&mut title_font_sz, 12..=64).suffix(" px")).changed() {
                                    changed = true;
                                }
                            }
                        });

                        ui.add_space(4.0);
                        ui.label(RichText::new("Message / Annonce du Live :").strong().color(Color32::WHITE));
                        if ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(ui.available_width())).changed() {
                            changed = true;
                        }

                        ui.add_space(6.0);

                        // Typographie & Alignement
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Police :").size(11.5).color(Color32::from_rgb(203, 213, 225)));
                            if ui.add(egui::Slider::new(&mut font_sz, 10..=80).suffix(" px")).changed() {
                                changed = true;
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Alignement :").size(11.5).color(Color32::from_rgb(203, 213, 225)));
                            if ui.selectable_value(&mut align, "left".to_string(), "⬅️ Gauche").clicked()
                                || ui.selectable_value(&mut align, "center".to_string(), "↔️ Centré").clicked()
                                || ui.selectable_value(&mut align, "right".to_string(), "➡️ Droite").clicked()
                            {
                                changed = true;
                            }
                        });

                        ui.add_space(4.0);

                        // Options de Mise en page avancée
                        ui.collapsing("📐 Mise en page avancée (Marges, Bordures, Opacité)", |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Marge interne :").size(11.0));
                                if ui.add(egui::Slider::new(&mut padding, 4..=48).suffix(" px")).changed() {
                                    changed = true;
                                }
                                ui.add_space(6.0);
                                ui.label(RichText::new("Opacité fond :").size(11.0));
                                if ui.add(egui::Slider::new(&mut bg_opac, 0..=100).suffix("%")).changed() {
                                    changed = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Bordure :").size(11.0));
                                if ui.add(egui::Slider::new(&mut border_w, 0..=12).suffix(" px")).changed() {
                                    changed = true;
                                }
                                ui.add_space(6.0);
                                ui.label(RichText::new("Arrondi coins :").size(11.0));
                                if ui.add(egui::Slider::new(&mut border_rad, 0..=32).suffix(" px")).changed() {
                                    changed = true;
                                }
                            });
                        });
                        ui.add_space(4.0);
                        // Styles rapides prédéfinis
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Styles :").size(11.0).color(Color32::GRAY));
                            if ui.small_button("⚡ Néon Violet").clicked() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_box_bg_color = "#15102a".to_string();
                                    st.custom_box_bg_opacity = 92;
                                    st.custom_box_border_color = "#a855f7".to_string();
                                    st.custom_box_border_width = 2;
                                    st.custom_box_border_radius = 12;
                                    st.custom_box_text_color = "#ffffff".to_string();
                                    save_state_json(&st);
                                }
                            }
                            if ui.small_button("🚨 Alerte Rouge").clicked() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_box_bg_color = "#2a0f12".to_string();
                                    st.custom_box_bg_opacity = 95;
                                    st.custom_box_border_color = "#ef4444".to_string();
                                    st.custom_box_border_width = 3;
                                    st.custom_box_border_radius = 8;
                                    st.custom_box_text_color = "#fee2e2".to_string();
                                    save_state_json(&st);
                                }
                            }
                            if ui.small_button("📜 Dark Épuré").clicked() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_box_bg_color = "#0b0d14".to_string();
                                    st.custom_box_bg_opacity = 96;
                                    st.custom_box_border_color = "#334155".to_string();
                                    st.custom_box_border_width = 1;
                                    st.custom_box_border_radius = 6;
                                    st.custom_box_text_color = "#f8fafc".to_string();
                                    save_state_json(&st);
                                }
                            }
                            if ui.small_button("🪟 Flou Furtif").clicked() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_box_bg_color = "#000000".to_string();
                                    st.custom_box_bg_opacity = 50;
                                    st.custom_box_border_color = "#64748b".to_string();
                                    st.custom_box_border_width = 1;
                                    st.custom_box_border_radius = 10;
                                    st.custom_box_text_color = "#ffffff".to_string();
                                    save_state_json(&st);
                                }
                            }
                        });

                        if changed {
                            if let Ok(mut st) = self.state.lock() {
                                st.custom_box_visible = box_vis;
                                st.custom_box_has_title = has_title;
                                st.custom_box_title = title;
                                st.custom_box_title_font_size = title_font_sz;
                                st.custom_box_text = text;
                                st.custom_box_font_size = font_sz;
                                st.custom_box_align = align;
                                st.custom_box_bg_opacity = bg_opac;
                                st.custom_box_border_width = border_w;
                                st.custom_box_border_radius = border_rad;
                                st.custom_box_padding = padding;
                                save_state_json(&st);
                            }
                        }
                    });


                ui.add_space(8.0);

        ui.add_space(6.0);
                // =========================================================
                // 2. BIBLIOTHÈQUE DE VIDÉOS & TAILLE ULTRA-RÉDUCTIBLE
                // =========================================================
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(24, 26, 34))
                    .stroke(Stroke::new(1.5_f32, Color32::from_rgb(168, 85, 247)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        let (mut vid_active, mut vid_scale, _vid_title, _vid_anti, mut vid_vol, vid_path) = {
                            let st = self.state.lock().unwrap();
                            (
                                st.video_active,
                                st.video_scale_pct,
                                st.video_title.clone(),
                                st.video_anti_detect,
                                st.video_volume,
                                st.video_file_path.clone(),
                            )
                        };

                        let (vid_src_type, current_yt_id, mut yt_auto, mut yt_loop, mut yt_ctrl, mut yt_mute) = {
                            let st = self.state.lock().unwrap();
                            (
                                st.video_source_type.clone(),
                                st.video_youtube_id.clone(),
                                st.video_yt_autoplay,
                                st.video_yt_loop,
                                st.video_yt_controls,
                                st.video_yt_mute,
                            )
                        };

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("🎬 2. LECTEUR VIDÉO DU LIVE (YOUTUBE & FICHIERS)")
                                    .strong()
                                    .color(Color32::from_rgb(216, 180, 254)),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let is_yt = vid_src_type == "youtube";
                                if ui.selectable_label(is_yt, "🔴 Mode YouTube").clicked() {
                                    if let Ok(mut st) = self.state.lock() {
                                        st.video_source_type = "youtube".to_string();
                                        save_state_json(&st);
                                    }
                                }
                                if ui.selectable_label(!is_yt, "📂 Fichiers Locaux").clicked() {
                                    if let Ok(mut st) = self.state.lock() {
                                        st.video_source_type = "local".to_string();
                                        save_state_json(&st);
                                    }
                                }
                            });
                        });
                        ui.add_space(5.0);

                        let toggle_col = if vid_active {
                            Color32::from_rgb(22, 163, 74)
                        } else {
                            Color32::from_rgb(55, 60, 75)
                        };
                        let toggle_txt = if vid_active {
                            "📺 [F12] VIDÉO AFFICHÉE SUR LE LIVE (Cliquer pour Masquer)"
                        } else {
                            "📺 [F12] AFFICHER LA VIDÉO SUR LE LIVE (Actuellement masquée)"
                        };

                        if ui
                            .add_sized(
                                [ui.available_width(), 32.0],
                                egui::Button::new(RichText::new(toggle_txt).strong().color(Color32::WHITE))
                                    .fill(toggle_col),
                            )
                            .clicked()
                        {
                            vid_active = !vid_active;
                            if let Ok(mut st) = self.state.lock() {
                                st.video_active = vid_active;
                                st.branch_timestamp = now_secs();
                                save_state_json(&st);
                            }
                        }

                        // --- PANNEAU YOUTUBE DÉDIÉ ---
                        if vid_src_type == "youtube" {
                            ui.add_space(4.0);
                            egui::Frame::none()
                                .fill(Color32::from_rgb(28, 18, 24))
                                .stroke(Stroke::new(1.2_f32, Color32::from_rgb(239, 68, 68)))
                                .rounding(Rounding::same(6.0))
                                .inner_margin(egui::Margin::same(8.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("🔴 URL Vidéo YouTube :").strong().size(11.0).color(Color32::from_rgb(248, 113, 113)));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new("⌨️ Collez avec Ctrl+V").size(10.5).color(Color32::GRAY));
                                        });
                                    });

                                    ui.add_space(3.0);
                                    ui.horizontal(|ui| {
                                        let yt_edit = ui.add(
                                            egui::TextEdit::singleline(&mut self.video_input_yt_url)
                                                .hint_text("https://www.youtube.com/watch?v=... ou youtu.be/...")
                                                .desired_width(ui.available_width() - 95.0),
                                        );

                                        let yt_enter = yt_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                                        if yt_enter || ui
                                            .add(
                                                egui::Button::new(RichText::new("▶️ Lancer").strong().size(11.0).color(Color32::WHITE))
                                                    .fill(Color32::from_rgb(220, 38, 38)),
                                            )
                                            .clicked()
                                        {
                                            let extracted = extract_youtube_id(&self.video_input_yt_url);
                                            if !extracted.is_empty() {
                                                if let Ok(mut st) = self.state.lock() {
                                                    st.video_source_type = "youtube".to_string();
                                                    st.video_youtube_id = extracted.clone();
                                                    st.video_title = format!("🔴 YOUTUBE : {}", extracted);
                                                    st.video_active = true;
                                                    st.branch_timestamp = now_secs();
                                                    save_state_json(&st);
                                                }
                                            }
                                        }
                                    });

                                    if !current_yt_id.is_empty() {
                                        ui.add_space(3.0);
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(format!("🟢 Vidéo en cours : https://youtu.be/{}", current_yt_id)).size(10.5).color(Color32::from_rgb(74, 222, 128)));
                                            if ui.small_button("⏹️ Vider").clicked() {
                                                if let Ok(mut st) = self.state.lock() {
                                                    st.video_youtube_id = String::new();
                                                    st.video_active = false;
                                                    save_state_json(&st);
                                                }
                                            }
                                        });
                                    }

                                    ui.add_space(4.0);
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.checkbox(&mut yt_auto, "▶️ Lecture auto").changed() {
                                            if let Ok(mut st) = self.state.lock() { st.video_yt_autoplay = yt_auto; save_state_json(&st); }
                                        }
                                        if ui.checkbox(&mut yt_loop, "🔁 Boucle (Loop)").changed() {
                                            if let Ok(mut st) = self.state.lock() { st.video_yt_loop = yt_loop; save_state_json(&st); }
                                        }
                                        if ui.checkbox(&mut yt_ctrl, "🎮 Contrôles").changed() {
                                            if let Ok(mut st) = self.state.lock() { st.video_yt_controls = yt_ctrl; save_state_json(&st); }
                                        }
                                        if ui.checkbox(&mut yt_mute, "🔇 Silencieux").changed() {
                                            if let Ok(mut st) = self.state.lock() { st.video_yt_mute = yt_mute; save_state_json(&st); }
                                        }
                                    });
                                });
                        }

                        ui.add_space(5.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("📏 Taille vidéo :").strong().color(Color32::WHITE));
                            if ui
                                .add(egui::Slider::new(&mut vid_scale, 8..=60).suffix(" %"))
                                .changed()
                            {
                                if let Ok(mut st) = self.state.lock() {
                                    st.video_scale_pct = vid_scale;
                                    save_state_json(&st);
                                }
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Tailles rapides :").size(11.5).color(Color32::GRAY));
                            for (lbl, pct) in [
                                ("Mini Vignette (10%)", 10_u32),
                                ("Très Petit (16%)", 16),
                                ("Petit (24%)", 24),
                                ("Moyen (34%)", 34),
                                ("Grand (46%)", 46),
                            ] {
                                if ui.small_button(lbl).clicked() {
                                    if let Ok(mut st) = self.state.lock() {
                                        st.video_scale_pct = pct;
                                        save_state_json(&st);
                                    }
                                }
                            }
                        });

                        ui.add_space(5.0);

                        ui.horizontal(|ui| {
                            if ui
                                .button(RichText::new("📁 Choisir un Dossier Bibliothèque...").strong())
                                .clicked()
                            {
                                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                    self.library.folder_path = folder.to_string_lossy().to_string();
                                    self.scan_library_folder();
                                }
                            }

                            if ui
                                .button(RichText::new("➕ Ajouter des vidéos (MP4/WebM)...").strong())
                                .clicked()
                            {
                                if let Some(files) = rfd::FileDialog::new()
                                    .add_filter("Vidéos (MP4, WebM, MKV, MOV)", &["mp4", "webm", "mkv", "mov"])
                                    .pick_files()
                                {
                                    for f in files {
                                        let s = f.to_string_lossy().to_string();
                                        if !self.library.files.contains(&s) {
                                            self.library.files.push(s);
                                        }
                                    }
                                    self.save_library();
                                    if let Some(first) = self.library.files.last().cloned() {
                                        self.play_library_video(&first);
                                    }
                                }
                            }

                            if ui.button("🔄 Actualiser").clicked() {
                                self.scan_library_folder();
                            }
                        });

                        ui.add_space(4.0);
                        egui::Frame::none()
                            .fill(Color32::from_rgb(15, 16, 21))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(45, 48, 60)))
                            .rounding(Rounding::same(6.0))
                            .inner_margin(egui::Margin::same(6.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "📚 Ta Bibliothèque ({} vidéo(s) — Dossier : {}) :",
                                        self.library.files.len(),
                                        self.library.folder_path
                                    ))
                                    .size(11.0)
                                    .color(Color32::from_rgb(156, 163, 175)),
                                );

                                if self.library.files.is_empty() {
                                    ui.label(
                                        RichText::new("Aucune vidéo dans la bibliothèque. Clique sur « ➕ Ajouter des vidéos » ou « 📁 Choisir un Dossier ».")
                                            .size(11.5)
                                            .color(Color32::from_rgb(203, 213, 225)),
                                    );
                                } else {
                                    let files_snapshot = self.library.files.clone();
                                    let mut to_play: Option<String> = None;
                                    let mut to_remove: Option<String> = None;

                                    egui::ScrollArea::vertical()
                                        .id_salt("video_lib_scroll")
                                        .max_height(105.0)
                                        .show(ui, |ui| {
                                            for full_p in &files_snapshot {
                                                let is_current = *full_p == vid_path;
                                                let short_name = Path::new(full_p)
                                                    .file_name()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or(full_p);

                                                ui.horizontal(|ui| {
                                                    let btn_col = if is_current {
                                                        Color32::from_rgb(147, 51, 234)
                                                    } else {
                                                        Color32::from_rgb(32, 35, 45)
                                                    };
                                                    let icon = if is_current { "🟢 EN COURS :" } else { "▶️ Lancer :" };
                                                    if ui
                                                        .add_sized(
                                                            [ui.available_width() - 34.0, 24.0],
                                                            egui::Button::new(
                                                                RichText::new(format!("{} {}", icon, short_name))
                                                                    .size(12.0)
                                                                    .strong()
                                                                    .color(Color32::WHITE),
                                                            )
                                                            .fill(btn_col),
                                                        )
                                                        .clicked()
                                                    {
                                                        to_play = Some(full_p.clone());
                                                    }
                                                    if ui.small_button("✖").on_hover_text("Retirer de la liste").clicked() {
                                                        to_remove = Some(full_p.clone());
                                                    }
                                                });
                                            }
                                        });

                                    if let Some(p) = to_play {
                                        self.play_library_video(&p);
                                    }
                                    if let Some(rem) = to_remove {
                                        self.library.files.retain(|x| x != &rem);
                                        self.save_library();
                                    }
                                }
                            });

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("⏯️ Play / Pause Vidéo").clicked() {
                                self.send_video_cmd("toggle_play");
                            }
                            if ui.button("⏪ -10s").clicked() {
                                self.send_video_cmd("seek_back");
                            }
                            if ui.button("⏩ +10s").clicked() {
                                self.send_video_cmd("seek_fwd");
                            }
                            if ui.button("🔄 Début").clicked() {
                                self.send_video_cmd("restart");
                            }
                            ui.label("🔊 Vol:");
                            if ui.add(egui::Slider::new(&mut vid_vol, 0..=100).suffix("%")).changed() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.video_volume = vid_vol;
                                    save_state_json(&st);
                                }
                            }
                        });
                    });

                ui.add_space(8.0);

        ui.add_space(6.0);
                // =========================================================
                // 3. SON DÉTECTÉ SUR LE PC (YOUTUBE, SPOTIFY, NAVIGATEUR...)
                // =========================================================
                let track = {
                    let st = self.state.lock().unwrap();
                    st.spotify.clone()
                };
                let peak = f32::from_bits(self.audio_peak_atomic.load(std::sync::atomic::Ordering::Relaxed));

                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(24, 26, 34))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(34, 197, 94)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("🔊 3. SON & MUSIQUE DÉTECTÉS SUR LE PC (YouTube, Spotify, VLC...)")
                                .strong()
                                .color(Color32::from_rgb(74, 222, 128)),
                        );
                        ui.add_space(3.0);

                        if track.connected {
                            let icon = if track.playing { "▶️" } else { "⏸️" };
                            ui.label(
                                RichText::new(format!("{} {}", icon, track.title))
                                    .strong()
                                    .size(14.0)
                                    .color(Color32::WHITE),
                            );
                            ui.label(
                                RichText::new(format!("🎤 {}   •   💿 {}", track.artist, track.album))
                                    .size(12.0)
                                    .color(Color32::from_rgb(203, 213, 225)),
                            );
                        } else {
                            ui.label("En attente d'un son sur YouTube, Spotify ou ton PC...");
                        }

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Niveau Audio Carte Son :").size(11.0));
                            ui.add(
                                egui::ProgressBar::new(peak)
                                    .desired_width(180.0)
                                    .fill(Color32::from_rgb(249, 115, 22)),
                            );
                            if ui.small_button("⏮️").clicked() {
                                trigger_media_prev();
                            }
                            if ui.small_button("⏯️ Play/Pause").clicked() {
                                trigger_media_play_pause();
                            }
                            if ui.small_button("⏭️").clicked() {
                                trigger_media_next();
                            }
                        });
                    });

                ui.add_space(8.0);

    }

    fn render_tab_chat(&mut self, ui: &mut egui::Ui) {
                // =========================================================
                // 6. CHAT TIKTOK LIVE EN DIRECT & BOT MODÉRATEUR
                // =========================================================
                egui::Frame::none()
                    .fill(Color32::from_rgb(18, 21, 30))
                    .stroke(Stroke::new(1.2_f32, Color32::from_rgb(37, 99, 235)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        let (mut handle, status, mut auto_emerg, pinned_vis, pinned_author, pinned_txt) = {
                            let st = self.state.lock().unwrap();
                            (
                                st.chat_handle.clone(),
                                st.chat_status.clone(),
                                st.bot_auto_emergency,
                                st.pinned_visible,
                                st.pinned_author.clone(),
                                st.pinned_text.clone(),
                            )
                        };

                        // 1. En-tête de la Section avec Badge et Indicateur de Connexion
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("💬 6. CHAT TIKTOK LIVE & BOT MODÉRATEUR")
                                    .strong()
                                    .size(13.5)
                                    .color(Color32::from_rgb(147, 197, 253)),
                            );
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("⚙️ Configurer le Bot...")
                                            .size(11.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(37, 99, 235)),
                                )
                                .clicked()
                            {
                                self.show_bot_config_window = true;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (status_col, status_icon) = if status.to_lowercase().contains("connect") {
                                    (Color32::from_rgb(34, 197, 94), "🟢")
                                } else if status.to_lowercase().contains("attente") || status.to_lowercase().contains("connexion") {
                                    (Color32::from_rgb(234, 179, 8), "🟡")
                                } else {
                                    (Color32::from_rgb(156, 163, 175), "⚪")
                                };
                                ui.label(
                                    RichText::new(format!("{} {}", status_icon, status))
                                        .size(11.0)
                                        .color(status_col),
                                );
                            });
                        });

                        ui.add_space(6.0);

                        // 2. Ligne de Connexion du compte TikTok
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Pseudo TikTok :")
                                    .strong()
                                    .color(Color32::from_rgb(226, 232, 240)),
                            );

                            let handle_edit = egui::TextEdit::singleline(&mut handle)
                                .hint_text(RichText::new("@ton_pseudo").color(Color32::from_rgb(100, 116, 139)))
                                .desired_width(170.0)
                                .text_color(Color32::WHITE);
                            if ui.add(handle_edit).changed() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.chat_handle = handle.clone();
                                    save_state_json(&st);
                                }
                            }

                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("⚡ Connecter au Live")
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(79, 70, 229)),
                                )
                                .clicked()
                            {
                                self.start_tiktok_chat_bridge();
                            }
                        });

                        ui.add_space(4.0);

                        // 3. Toggles de modération & d'affichage
                        ui.horizontal(|ui| {
                            if ui
                                .checkbox(
                                    &mut auto_emerg,
                                    RichText::new("🚨 Auto-Urgences (Affiche les N° d'aide en direct)")
                                        .color(Color32::from_rgb(248, 113, 113)),
                                )
                                .changed()
                            {
                                if let Ok(mut st) = self.state.lock() {
                                    st.bot_auto_emergency = auto_emerg;
                                    save_state_json(&st);
                                }
                            }
                            ui.checkbox(
                                &mut self.chat_only_questions,
                                RichText::new("❓ Questions uniquement")
                                    .color(Color32::from_rgb(56, 189, 248)),
                            );
                        });

                        ui.add_space(6.0);

                        // Contrôles typographiques du commentaire épinglé
                        ui.horizontal(|ui| {
                            let (mut p_sz, mut p_align) = {
                                let st = self.state.lock().unwrap();
                                (st.pinned_font_size, st.pinned_align.clone())
                            };
                            ui.label(RichText::new("Taille Épinglé :").size(11.5).color(Color32::from_rgb(203, 213, 225)));
                            if ui.add(egui::Slider::new(&mut p_sz, 12..=64).suffix(" px")).changed() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.pinned_font_size = p_sz;
                                    save_state_json(&st);
                                }
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Alignement :").size(11.5).color(Color32::from_rgb(203, 213, 225)));
                            if ui.selectable_value(&mut p_align, "left".to_string(), "⬅️ Gauche").clicked()
                                || ui.selectable_value(&mut p_align, "center".to_string(), "↔️ Centré").clicked()
                                || ui.selectable_value(&mut p_align, "right".to_string(), "➡️ Droite").clicked()
                            {
                                if let Ok(mut st) = self.state.lock() {
                                    st.pinned_align = p_align;
                                    save_state_json(&st);
                                }
                            }
                        });
                        ui.add_space(4.0);

                        // 4. Commentaire actuellement épinglé sur l'écran Live
                        if pinned_vis && !pinned_txt.is_empty() {
                            egui::Frame::none()
                                .fill(Color32::from_rgb(34, 21, 56))
                                .stroke(Stroke::new(1.5_f32, Color32::from_rgb(168, 85, 247)))
                                .rounding(Rounding::same(6.0))
                                .inner_margin(egui::Margin::same(8.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("📌 ÉPINGLÉ SUR LE LIVE : {}", pinned_author))
                                                .strong()
                                                .color(Color32::from_rgb(216, 180, 254)),
                                        );
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new("❌ Retirer de l'écran")
                                                            .size(11.0)
                                                            .color(Color32::WHITE),
                                                    )
                                                    .fill(Color32::from_rgb(147, 51, 234)),
                                                )
                                                .clicked()
                                            {
                                                if let Ok(mut st) = self.state.lock() {
                                                    st.pinned_visible = false;
                                                    save_state_json(&st);
                                                }
                                            }
                                        });
                                    });
                                    ui.label(
                                        RichText::new(&pinned_txt)
                                            .strong()
                                            .size(12.5)
                                            .color(Color32::WHITE),
                                    );
                                });
                            ui.add_space(6.0);
                        }

                        // 5. Flux de Commentaires TikTok (Style Dark Fantasy avec bulles de messages)
                        let comments_snapshot = {
                            let st = self.state.lock().unwrap();
                            st.chat_comments.clone()
                        };

                        let filtered: Vec<_> = comments_snapshot
                            .iter()
                            .rev()
                            .filter(|c| !self.chat_only_questions || c.is_question)
                            .take(15)
                            .collect();

                        egui::Frame::none()
                            .fill(Color32::from_rgb(13, 15, 22))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(30, 38, 54)))
                            .rounding(Rounding::same(6.0))
                            .inner_margin(egui::Margin::same(6.0))
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("chat_comments_scroll")
                                    .max_height(140.0)
                                    .show(ui, |ui| {
                                        if filtered.is_empty() {
                                            ui.vertical_centered(|ui| {
                                                ui.add_space(10.0);
                                                ui.label(
                                                    RichText::new("💬 En attente de commentaires sur le Live TikTok...")
                                                        .size(12.0)
                                                        .color(Color32::from_rgb(148, 163, 184)),
                                                );
                                                ui.label(
                                                    RichText::new("Connecte ton pseudo ci-dessus pour afficher le chat en temps réel.")
                                                        .size(10.5)
                                                        .color(Color32::from_rgb(100, 116, 139)),
                                                );
                                                ui.add_space(10.0);
                                            });
                                        } else {
                                            for c in filtered {
                                                egui::Frame::none()
                                                    .fill(if c.is_question {
                                                        Color32::from_rgb(32, 28, 18)
                                                    } else if c.is_help_trigger {
                                                        Color32::from_rgb(38, 18, 22)
                                                    } else {
                                                        Color32::from_rgb(22, 26, 36)
                                                    })
                                                    .stroke(Stroke::new(
                                                        1.0_f32,
                                                        if c.is_question {
                                                            Color32::from_rgb(234, 179, 8)
                                                        } else if c.is_help_trigger {
                                                            Color32::from_rgb(239, 68, 68)
                                                        } else {
                                                            Color32::from_rgb(39, 48, 68)
                                                        },
                                                    ))
                                                    .rounding(Rounding::same(4.0))
                                                    .inner_margin(egui::Margin::symmetric(6.0, 4.0))
                                                    .show(ui, |ui| {
                                                        ui.horizontal(|ui| {
                                                            let (tag, tag_col) = if c.is_question {
                                                                ("❓ Question", Color32::from_rgb(250, 204, 21))
                                                            } else if c.is_help_trigger {
                                                                ("🚨 SOS", Color32::from_rgb(248, 113, 113))
                                                            } else {
                                                                ("💬", Color32::from_rgb(56, 189, 248))
                                                            };

                                                            ui.label(
                                                                RichText::new(format!("{} {}:", tag, c.author))
                                                                    .strong()
                                                                    .size(11.5)
                                                                    .color(tag_col),
                                                            );
                                                            ui.label(
                                                                RichText::new(&c.text)
                                                                    .size(12.0)
                                                                    .color(Color32::from_rgb(241, 245, 249)),
                                                            );

                                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                if ui
                                                                    .add(
                                                                        egui::Button::new(
                                                                            RichText::new("📌 Épingler")
                                                                                .size(10.5)
                                                                                .color(Color32::WHITE),
                                                                        )
                                                                        .fill(Color32::from_rgb(30, 58, 138)),
                                                                    )
                                                                    .clicked()
                                                                {
                                                                    if let Ok(mut st) = self.state.lock() {
                                                                        st.pinned_author = c.author.clone();
                                                                        st.pinned_text = c.text.clone();
                                                                        st.pinned_avatar = c.avatar.clone();
                                                                        st.pinned_visible = true;
                                                                        save_state_json(&st);
                                                                    }
                                                                }
                                                            });
                                                        });
                                                    });
                                                ui.add_space(2.0);
                                            }
                                        }
                                    });
                            });
                    });

                ui.add_space(8.0);

        ui.add_space(6.0);
                // =========================================================
                // 8. JUKEBOX & VOTES DU CHAT (DEMANDES MUSICALES)
                // =========================================================
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(18, 22, 32))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(168, 85, 247)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        let (mut song_mode, current_requester, reqs, mut vote_st) = {
                            let st = self.state.lock().unwrap();
                            (
                                st.song_mode.clone(),
                                st.current_song_requester.clone(),
                                st.song_requests.clone(),
                                st.song_vote.clone(),
                            )
                        };

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("🎵 8. JUKEBOX & VOTES MUSICALES DU CHAT")
                                    .strong()
                                    .color(Color32::from_rgb(216, 180, 254)),
                            );
                            if song_mode != "off" {
                                ui.colored_label(Color32::from_rgb(74, 222, 128), "● ACTIF");
                            } else {
                                ui.colored_label(Color32::GRAY, "○ DÉSACTIVÉ");
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Mode :");
                            if ui.selectable_value(&mut song_mode, "off".to_string(), "🔒 Désactivé").changed()
                                || ui.selectable_value(&mut song_mode, "queue".to_string(), "📋 File d'Attente (!musique)").changed()
                                || ui.selectable_value(&mut song_mode, "vote".to_string(), "🗳️ Vote Démocratique (!1/!2/!3)").changed()
                            {
                                if let Ok(mut st) = self.state.lock() {
                                    st.song_mode = song_mode.clone();
                                    save_state_json(&st);
                                }
                            }
                        });

                        // Dédicace en cours
                        ui.horizontal(|ui| {
                            ui.label("Dédicace sur Scène :");
                            if !current_requester.is_empty() {
                                ui.colored_label(Color32::from_rgb(250, 204, 21), format!("🎧 Demandé par {}", current_requester));
                                if ui.small_button("❌ Effacer").clicked() {
                                    if let Ok(mut st) = self.state.lock() {
                                        st.current_song_requester = String::new();
                                        save_state_json(&st);
                                    }
                                }
                            } else {
                                ui.label(RichText::new("Aucune dédicace active").italics().color(Color32::GRAY));
                            }
                        });

                        ui.separator();

                        if song_mode == "queue" {
                            ui.label(RichText::new("💡 Les spectateurs tapent dans le chat : !musique [Titre] ou !sr [Titre]").size(11.0).color(Color32::from_rgb(147, 197, 253)));

                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("File d'attente ({} demandes) :", reqs.len())).strong().color(Color32::WHITE));
                                if !reqs.is_empty() && ui.small_button("🗑️ Tout Vider").clicked() {
                                    if let Ok(mut st) = self.state.lock() {
                                        st.song_requests.clear();
                                        save_state_json(&st);
                                    }
                                }
                            });

                            if reqs.is_empty() {
                                ui.label(RichText::new("⏳ En attente de demandes du chat...").italics().color(Color32::GRAY));
                            } else {
                                egui::ScrollArea::vertical()
                                    .max_height(140.0)
                                    .id_salt("song_requests_scroll")
                                    .show(ui, |ui| {
                                        let mut to_remove_id: Option<u64> = None;
                                        for req_item in &reqs {
                                            ui.horizontal(|ui| {
                                                ui.colored_label(Color32::from_rgb(192, 132, 252), format!("@{}:", req_item.author));
                                                ui.label(RichText::new(&req_item.query).strong().color(Color32::WHITE));

                                                if ui.button("▶ Spotify").clicked() {
                                                    let _ = Command::new("cmd")
                                                        .args(["/C", "start", "", &format!("spotify:search:{}", req_item.query)])
                                                        .spawn();
                                                    if let Ok(mut st) = self.state.lock() {
                                                        st.current_song_requester = format!("@{}", req_item.author);
                                                        save_state_json(&st);
                                                    }
                                                    to_remove_id = Some(req_item.id);
                                                }

                                                if ui.button("▶ YouTube").clicked() {
                                                    let url = format!("https://www.youtube.com/results?search_query={}", req_item.query.replace(' ', "+"));
                                                    let _ = Command::new("cmd").args(["/C", "start", "", &url]).spawn();
                                                    if let Ok(mut st) = self.state.lock() {
                                                        st.current_song_requester = format!("@{}", req_item.author);
                                                        save_state_json(&st);
                                                    }
                                                    to_remove_id = Some(req_item.id);
                                                }

                                                if ui.button("❌").clicked() {
                                                    to_remove_id = Some(req_item.id);
                                                }
                                            });
                                        }
                                        if let Some(rid) = to_remove_id {
                                            if let Ok(mut st) = self.state.lock() {
                                                st.song_requests.retain(|r| r.id != rid);
                                                save_state_json(&st);
                                            }
                                        }
                                    });
                            }
                        } else if song_mode == "vote" {
                            ui.label(RichText::new("💡 Les spectateurs votent en tapant !1, !2, !3 ou !4 dans le chat").size(11.0).color(Color32::from_rgb(147, 197, 253)));

                            let mut vote_changed = false;
                            ui.horizontal(|ui| {
                                ui.label("Titre / Question :");
                                vote_changed |= ui.text_edit_singleline(&mut vote_st.title).changed();
                            });

                            ui.horizontal(|ui| {
                                ui.label(RichText::new("!1").strong().color(Color32::from_rgb(239, 68, 68)));
                                vote_changed |= ui.text_edit_singleline(&mut vote_st.choice1).changed();
                                ui.colored_label(Color32::WHITE, format!("({} votes)", vote_st.votes1));
                            });

                            ui.horizontal(|ui| {
                                ui.label(RichText::new("!2").strong().color(Color32::from_rgb(59, 130, 246)));
                                vote_changed |= ui.text_edit_singleline(&mut vote_st.choice2).changed();
                                ui.colored_label(Color32::WHITE, format!("({} votes)", vote_st.votes2));
                            });

                            ui.horizontal(|ui| {
                                ui.label(RichText::new("!3").strong().color(Color32::from_rgb(168, 85, 247)));
                                vote_changed |= ui.text_edit_singleline(&mut vote_st.choice3).changed();
                                ui.colored_label(Color32::WHITE, format!("({} votes)", vote_st.votes3));
                            });

                            ui.horizontal(|ui| {
                                ui.label(RichText::new("!4").strong().color(Color32::from_rgb(234, 179, 8)));
                                vote_changed |= ui.text_edit_singleline(&mut vote_st.choice4).changed();
                                ui.colored_label(Color32::WHITE, format!("({} votes)", vote_st.votes4));
                            });

                            let total_votes = vote_st.votes1 + vote_st.votes2 + vote_st.votes3 + vote_st.votes4;
                            ui.label(RichText::new(format!("Total des votes : {} ({} votants uniques)", total_votes, vote_st.voters.len())).size(11.5).color(Color32::from_rgb(209, 213, 219)));

                            ui.horizontal(|ui| {
                                if !vote_st.active {
                                    if ui.add(egui::Button::new(RichText::new("▶ Lancer le Vote sur la Scène").strong().color(Color32::WHITE))
                                        .fill(Color32::from_rgb(22, 163, 74)))
                                        .clicked()
                                    {
                                        vote_st.active = true;
                                        vote_st.votes1 = 0;
                                        vote_st.votes2 = 0;
                                        vote_st.votes3 = 0;
                                        vote_st.votes4 = 0;
                                        vote_st.voters.clear();
                                        vote_st.winner = None;
                                        vote_changed = true;
                                    }
                                } else {
                                    if ui.add(egui::Button::new(RichText::new("⏹️ Clôturer le Vote").strong().color(Color32::WHITE))
                                        .fill(Color32::from_rgb(220, 38, 38)))
                                        .clicked()
                                    {
                                        vote_st.active = false;
                                        // Déterminer le gagnant
                                        let max_v = vote_st.votes1.max(vote_st.votes2).max(vote_st.votes3).max(vote_st.votes4);
                                        if max_v > 0 {
                                            if vote_st.votes1 == max_v { vote_st.winner = Some(1); }
                                            else if vote_st.votes2 == max_v { vote_st.winner = Some(2); }
                                            else if vote_st.votes3 == max_v { vote_st.winner = Some(3); }
                                            else if vote_st.votes4 == max_v { vote_st.winner = Some(4); }
                                        }
                                        vote_changed = true;
                                    }
                                }

                                if ui.button("🔄 Réinitialiser les scores").clicked() {
                                    vote_st.votes1 = 0;
                                    vote_st.votes2 = 0;
                                    vote_st.votes3 = 0;
                                    vote_st.votes4 = 0;
                                    vote_st.voters.clear();
                                    vote_st.winner = None;
                                    vote_changed = true;
                                }

                                if let Some(w) = vote_st.winner {
                                    let win_title = match w {
                                        1 => &vote_st.choice1,
                                        2 => &vote_st.choice2,
                                        3 => &vote_st.choice3,
                                        4 => &vote_st.choice4,
                                        _ => "",
                                    };
                                    ui.colored_label(Color32::from_rgb(250, 204, 21), format!("🏆 Gagnant : Choix {} ({})", w, win_title));
                                    if ui.button("▶ Lancer Gagnant (Spotify)").clicked() {
                                        let _ = Command::new("cmd").args(["/C", "start", "", &format!("spotify:search:{}", win_title)]).spawn();
                                        if let Ok(mut st) = self.state.lock() {
                                            st.current_song_requester = "Vote de la Communauté 👑".to_string();
                                            save_state_json(&st);
                                        }
                                    }
                                }
                            });

                            if vote_changed {
                                if let Ok(mut st) = self.state.lock() {
                                    st.song_vote = vote_st;
                                    save_state_json(&st);
                                }
                            }
                        }
                    });
    }

    fn render_tab_bannieres(&mut self, ui: &mut egui::Ui) {
        let mut emerg_vis = if let Ok(st) = self.state.lock() { st.emergency_visible } else { false };
                // =========================================================
                // 7. ENCART TEXTE LIBRE SUR LE LIVE (BANNIÈRE SUR-MESURE)
                // =========================================================
                egui::Frame::none()
                    .fill(Color32::from_rgb(20, 24, 34))
                    .stroke(Stroke::new(1.2_f32, Color32::from_rgb(168, 85, 247)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        let (mut box_vis, mut has_title, mut title, mut title_font_sz, mut text, mut font_sz, mut align, mut bg_opac, mut border_w, mut border_rad, mut padding) = {
                            let st = self.state.lock().unwrap();
                            (
                                st.custom_box_visible,
                                st.custom_box_has_title,
                                st.custom_box_title.clone(),
                                st.custom_box_title_font_size,
                                st.custom_box_text.clone(),
                                st.custom_box_font_size,
                                st.custom_box_align.clone(),
                                st.custom_box_bg_opacity,
                                st.custom_box_border_width,
                                st.custom_box_border_radius,
                                st.custom_box_padding,
                            )
                        };

                        let mut changed = false;

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("📢 7. ENCART TEXTE LIBRE SUR LE LIVE (Bannière Sur-Mesure)")
                                    .strong()
                                    .size(13.5)
                                    .color(Color32::from_rgb(216, 180, 254)),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let btn_col = if box_vis {
                                    Color32::from_rgb(22, 163, 74)
                                } else {
                                    Color32::from_rgb(75, 85, 99)
                                };
                                let btn_txt = if box_vis {
                                    "👁️ ENCART AFFICHÉ (Cliquer pour Masquer)"
                                } else {
                                    "👁️ AFFICHER L'ENCART SUR LE LIVE"
                                };
                                if ui.add(egui::Button::new(RichText::new(btn_txt).strong().color(Color32::WHITE)).fill(btn_col)).clicked() {
                                    box_vis = !box_vis;
                                    changed = true;
                                }
                            });
                        });

                        ui.add_space(6.0);

                        // Option Titre
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut has_title, RichText::new("🏷️ Activer un Titre").strong().color(Color32::from_rgb(147, 197, 253))).changed() {
                                changed = true;
                            }
                            if has_title {
                                ui.label(RichText::new("Titre :").color(Color32::WHITE));
                                if ui.add(egui::TextEdit::singleline(&mut title).desired_width(180.0)).changed() {
                                    changed = true;
                                }
                                ui.label(RichText::new("Taille titre :").size(11.0).color(Color32::from_rgb(203, 213, 225)));
                                if ui.add(egui::Slider::new(&mut title_font_sz, 12..=64).suffix(" px")).changed() {
                                    changed = true;
                                }
                            }
                        });

                        ui.add_space(4.0);
                        ui.label(RichText::new("Texte / Message :").strong().color(Color32::WHITE));
                        if ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(ui.available_width())).changed() {
                            changed = true;
                        }

                        ui.add_space(6.0);

                        // Typographie & Alignement
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Taille Police :").size(12.0).color(Color32::from_rgb(203, 213, 225)));
                            if ui.add(egui::Slider::new(&mut font_sz, 10..=80).suffix(" px")).changed() {
                                changed = true;
                            }
                            ui.add_space(10.0);
                            ui.label(RichText::new("Alignement :").size(12.0).color(Color32::from_rgb(203, 213, 225)));
                            if ui.selectable_value(&mut align, "left".to_string(), "⬅️ Gauche").clicked()
                                || ui.selectable_value(&mut align, "center".to_string(), "↔️ Centré").clicked()
                                || ui.selectable_value(&mut align, "right".to_string(), "➡️ Droite").clicked()
                            {
                                changed = true;
                            }
                        });

                        ui.add_space(4.0);

                        // Options du Cadre (Fond, Opacité, Bordure, Arrondis, Marge)
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Opacité :").size(12.0).color(Color32::from_rgb(203, 213, 225)));
                            if ui.add(egui::Slider::new(&mut bg_opac, 0..=100).suffix("%")).changed() {
                                changed = true;
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Bordure :").size(12.0).color(Color32::from_rgb(203, 213, 225)));
                            if ui.add(egui::Slider::new(&mut border_w, 0..=8).suffix(" px")).changed() {
                                changed = true;
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Coins :").size(12.0).color(Color32::from_rgb(203, 213, 225)));
                            if ui.add(egui::Slider::new(&mut border_rad, 0..=25).suffix(" px")).changed() {
                                changed = true;
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new("Marge :").size(12.0).color(Color32::from_rgb(203, 213, 225)));
                            if ui.add(egui::Slider::new(&mut padding, 4..=30).suffix(" px")).changed() {
                                changed = true;
                            }
                        });

                        // Styles rapides prédéfinis
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Styles rapides :").size(11.0).color(Color32::GRAY));
                            if ui.small_button("⚡ Néon Violet").clicked() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_box_bg_color = "#15102a".to_string();
                                    st.custom_box_bg_opacity = 92;
                                    st.custom_box_border_color = "#a855f7".to_string();
                                    st.custom_box_border_width = 2;
                                    st.custom_box_border_radius = 12;
                                    st.custom_box_text_color = "#ffffff".to_string();
                                    save_state_json(&st);
                                }
                            }
                            if ui.small_button("🚨 Alerte Rouge").clicked() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_box_bg_color = "#2a0f12".to_string();
                                    st.custom_box_bg_opacity = 95;
                                    st.custom_box_border_color = "#ef4444".to_string();
                                    st.custom_box_border_width = 3;
                                    st.custom_box_border_radius = 8;
                                    st.custom_box_text_color = "#fee2e2".to_string();
                                    save_state_json(&st);
                                }
                            }
                            if ui.small_button("📜 Dark Épuré").clicked() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_box_bg_color = "#0b0d14".to_string();
                                    st.custom_box_bg_opacity = 96;
                                    st.custom_box_border_color = "#334155".to_string();
                                    st.custom_box_border_width = 1;
                                    st.custom_box_border_radius = 6;
                                    st.custom_box_text_color = "#f8fafc".to_string();
                                    save_state_json(&st);
                                }
                            }
                            if ui.small_button("🪟 Flou Furtif").clicked() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_box_bg_color = "#000000".to_string();
                                    st.custom_box_bg_opacity = 50;
                                    st.custom_box_border_color = "#64748b".to_string();
                                    st.custom_box_border_width = 1;
                                    st.custom_box_border_radius = 10;
                                    st.custom_box_text_color = "#ffffff".to_string();
                                    save_state_json(&st);
                                }
                            }
                        });

                        if changed {
                            if let Ok(mut st) = self.state.lock() {
                                st.custom_box_visible = box_vis;
                                st.custom_box_has_title = has_title;
                                st.custom_box_title = title;
                                st.custom_box_title_font_size = title_font_sz;
                                st.custom_box_text = text;
                                st.custom_box_font_size = font_sz;
                                st.custom_box_align = align;
                                st.custom_box_bg_opacity = bg_opac;
                                st.custom_box_border_width = border_w;
                                st.custom_box_border_radius = border_rad;
                                st.custom_box_padding = padding;
                                save_state_json(&st);
                            }
                        }
                    });

                ui.add_space(8.0);

        ui.add_space(6.0);
                // =========================================================
                // 5. BANDEAU TRIGGER WARNING (SUJET SENSIBLE)
                // =========================================================
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(28, 20, 24))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(239, 68, 68)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        let (mut tw_vis, mut tw_txt) = {
                            let st = self.state.lock().unwrap();
                            (st.trigger_warning_visible, st.trigger_warning_text.clone())
                        };

                        ui.horizontal(|ui| {
                            if ui
                                .checkbox(
                                    &mut tw_vis,
                                    RichText::new("⚠️ Avertissement Sensible (Trigger Warning) sur le Live")
                                        .strong()
                                        .color(Color32::from_rgb(252, 165, 165)),
                                )
                                .changed()
                            {
                                if let Ok(mut st) = self.state.lock() {
                                    st.trigger_warning_visible = tw_vis;
                                    save_state_json(&st);
                                }
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Texte TW :");
                            if ui.text_edit_singleline(&mut tw_txt).changed() {
                                if let Ok(mut st) = self.state.lock() {
                                    st.trigger_warning_text = tw_txt;
                                    save_state_json(&st);
                                }
                            }
                        });
                    });

                ui.add_space(8.0);

        ui.add_space(6.0);

        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(24, 26, 34))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(147, 51, 234)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("📞 NUMÉROS D'URGENCE VSS / INCESTE (Affichage & Style)")
                        .strong()
                        .color(Color32::from_rgb(216, 180, 254)),
                );
                ui.add_space(4.0);
                            // Barre de contrôle rapide du Panneau Numéros d'Urgence
                            ui.horizontal(|ui| {
                                if ui
                                    .checkbox(
                                        &mut emerg_vis,
                                        RichText::new("🚨 Afficher le Grand Panneau des Numéros d'Urgence sur le Live")
                                            .strong()
                                            .color(Color32::from_rgb(216, 180, 254)),
                                    )
                                    .changed()
                                {
                                    if let Ok(mut st) = self.state.lock() {
                                        st.emergency_visible = emerg_vis;
                                        save_state_json(&st);
                                    }
                                }

                                if ui
                                    .button(
                                        RichText::new("⚙️ Configurer les Numéros d'Urgence...")
                                            .strong()
                                            .color(Color32::from_rgb(203, 213, 225)),
                                    )
                                    .clicked()
                                {
                                    self.show_emergency_config_window = true;
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Format 16:9 :").size(11.5).strong().color(Color32::from_rgb(203, 213, 225)));
                                let (mut pos_16_9, mut scale_pct) = {
                                    let st = self.state.lock().unwrap();
                                    (st.emergency_pos_16_9.clone(), st.emergency_scale_pct)
                                };
                                if ui.selectable_value(&mut pos_16_9, "left".to_string(), "⬅️ Gauche (Rectangle)").clicked()
                                    || ui.selectable_value(&mut pos_16_9, "right".to_string(), "➡️ Droite (Rectangle)").clicked()
                                    || ui.selectable_value(&mut pos_16_9, "top".to_string(), "⬆️ Haut (Bandeau)").clicked()
                                {
                                    if let Ok(mut st) = self.state.lock() {
                                        st.emergency_pos_16_9 = pos_16_9;
                                        save_state_json(&st);
                                    }
                                }
                                ui.add_space(8.0);
                                ui.label(RichText::new("Taille :").size(11.5).color(Color32::from_rgb(203, 213, 225)));
                                if ui.add(egui::Slider::new(&mut scale_pct, 70..=160).suffix("%")).changed() {
                                    if let Ok(mut st) = self.state.lock() {
                                        st.emergency_scale_pct = scale_pct;
                                        save_state_json(&st);
                                    }
                                }
                            });
            });
    }


    fn render_tab_articles(&mut self, ui: &mut egui::Ui) {
        let (
            mut art_vis,
            mut art_title,
            mut art_source,
            mut art_date,
            mut art_text,
            mut art_img,
            mut art_show_img,
            mut art_font_title,
            mut art_font_text,
            mut art_align,
            mut art_text_col,
            mut art_title_col,
            mut art_badge_col,
            mut art_bg_col,
            mut art_bg_opac,
            mut art_border_col,
            mut art_border_w,
            mut art_border_rad,
            mut art_glow,
            mut art_auto_scroll,
            mut art_scroll_speed,
        ) = {
            let st = self.state.lock().unwrap();
            (
                st.article.visible,
                st.article.title.clone(),
                st.article.source.clone(),
                st.article.date.clone(),
                st.article.text.clone(),
                st.article.image_url.clone(),
                st.article.show_image,
                st.article.font_size_title,
                st.article.font_size_text,
                st.article.text_align.clone(),
                st.article.text_color.clone(),
                st.article.title_color.clone(),
                st.article.source_badge_color.clone(),
                st.article.bg_color.clone(),
                st.article.bg_opacity,
                st.article.border_color.clone(),
                st.article.border_width,
                st.article.border_radius,
                st.article.glow,
                st.article.auto_scroll,
                st.article.scroll_speed,
            )
        };

        let mut changed = false;

        // --- BARRE D'ACTION SUPÉRIEURE ---
        egui::Frame::none()
            .fill(Color32::from_rgb(18, 20, 28))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(45, 50, 68)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let vis_btn = egui::Button::new(
                        RichText::new(if art_vis { "👁️ ARTICLE : EN LIGNE" } else { "👁️ Article : Masqué" })
                            .strong()
                            .size(11.5)
                            .color(Color32::WHITE),
                    )
                    .fill(if art_vis { Color32::from_rgb(16, 185, 129) } else { Color32::from_rgb(55, 65, 81) });

                    if ui.add_sized([160.0, 28.0], vis_btn).clicked() {
                        art_vis = !art_vis;
                        changed = true;
                    }

                    if ui
                        .add_sized(
                            [190.0, 28.0],
                            egui::Button::new(
                                RichText::new("🪟 Ouvrir Fenêtre Dédiée")
                                    .strong()
                                    .size(11.5)
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(234, 88, 12)),
                        )
                        .clicked()
                    {
                        Self::open_article_standalone_window();
                    }
                });
            });

        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .id_salt("scroll_tab_articles")
            .show(ui, |ui| {
                // --- SECTION 1 : EXTRACTION MAGIQUE DEPUIS LE WEB ---
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(20, 24, 34))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(56, 189, 248)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("🔗 1. RÉCUPÉRER UN ARTICLE (PRESSE, FACEBOOK, MÉDIAS)")
                                .strong()
                                .color(Color32::from_rgb(56, 189, 248)),
                        );
                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            let url_edit = ui.add(
                                egui::TextEdit::singleline(&mut self.article_input_url)
                                    .hint_text("Collez ici l'adresse URL (ex: Midi Libre, Le Monde, Facebook)...")
                                    .desired_width(ui.available_width() - 95.0),
                            );

                            let enter_pressed = url_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                            if enter_pressed || ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("🪄 Extraire")
                                            .strong()
                                            .size(11.0)
                                            .color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(14, 165, 233)),
                                )
                                .clicked()
                            {
                                self.scrape_article_from_url(&self.article_input_url.clone());
                            }
                        });

                        let current_status = {
                            self.article_scrape_status.lock().unwrap().clone()
                        };
                        if !current_status.is_empty() {
                            ui.add_space(3.0);
                            let col = if current_status.starts_with("✅") {
                                Color32::from_rgb(74, 222, 128)
                            } else if current_status.starts_with("❌") {
                                Color32::from_rgb(239, 68, 68)
                            } else {
                                Color32::from_rgb(251, 191, 36)
                            };
                            ui.label(
                                RichText::new(&current_status)
                                    .size(11.0)
                                    .color(col),
                            );
                        }

                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Source rapide :").size(10.5).color(Color32::from_rgb(148, 163, 184)));
                            let presets = [
                                "📰 LE MONDE",
                                "📱 FACEBOOK",
                                "✊ MEDIAPART",
                                "🔴 URGENT",
                                "🕊️ TÉMOIGNAGE",
                                "📻 RADIO",
                            ];
                            for p in presets {
                                if ui.small_button(p).clicked() {
                                    art_source = p.to_string();
                                    changed = true;
                                }
                            }
                        });
                    });

                ui.add_space(8.0);

                // --- SECTION 2 : ÉDITION DU CONTENU ---
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(22, 26, 36))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(234, 179, 8)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("📝 2. CONTENU DE L'ARTICLE")
                                .strong()
                                .color(Color32::from_rgb(250, 204, 21)),
                        );
                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Source :").size(11.0));
                            if ui.text_edit_singleline(&mut art_source).changed() {
                                changed = true;
                            }
                            ui.label(RichText::new("Date :").size(11.0));
                            if ui.text_edit_singleline(&mut art_date).changed() {
                                changed = true;
                            }
                        });

                        ui.add_space(4.0);
                        ui.label(RichText::new("Titre de l'article :").size(11.0));
                        if ui.text_edit_singleline(&mut art_title).changed() {
                            changed = true;
                        }

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut art_show_img, "Afficher l'image").changed() {
                                changed = true;
                            }
                            ui.label(RichText::new("URL Image :").size(11.0));
                            if ui.text_edit_singleline(&mut art_img).changed() {
                                changed = true;
                            }
                        });

                        ui.add_space(4.0);
                        ui.label(RichText::new("Texte / Extrait / Résumé :").size(11.0));
                        if ui
                            .add(
                                egui::TextEdit::multiline(&mut art_text)
                                    .desired_width(ui.available_width())
                                    .desired_rows(6),
                            )
                            .changed()
                        {
                            changed = true;
                        }

                        // Détection automatique si l'utilisateur a collé un lien dans la zone de texte
                        let link_detected = art_text.trim().starts_with("http://") || art_text.trim().starts_with("https://");
                        if link_detected {
                            ui.add_space(5.0);
                            egui::Frame::none()
                                .fill(Color32::from_rgb(15, 30, 52))
                                .stroke(Stroke::new(1.5_f32, Color32::from_rgb(56, 189, 248)))
                                .rounding(Rounding::same(8.0))
                                .inner_margin(egui::Margin::same(10.0))
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("🪄 Lien Web détecté dans le contenu de l'article !").strong().color(Color32::from_rgb(56, 189, 248)));
                                        });
                                        ui.add_space(4.0);
                                        let btn = egui::Button::new(
                                            RichText::new("⚡ EXTRAIRE AUTOMATIQUEMENT ET CHARGER L'ARTICLE")
                                                .strong()
                                                .size(12.0)
                                                .color(Color32::WHITE),
                                        )
                                        .fill(Color32::from_rgb(14, 165, 233));
                                        if ui.add_sized([ui.available_width(), 32.0], btn).clicked() {
                                            let link = art_text.trim().to_string();
                                            self.article_input_url = link.clone();
                                            art_text = "⏳ Extraction de l'article en cours...".to_string();
                                            changed = true;
                                            self.scrape_article_from_url(&link);
                                        }
                                    });
                                });
                        }
                    });

                ui.add_space(8.0);

                // --- SECTION 3 : PERSONNALISATION DE LA FENÊTRE ---
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(24, 26, 34))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(168, 85, 247)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("🎨 3. PERSONNALISATION COMPLÈTE DE LA FENÊTRE")
                                .strong()
                                .color(Color32::from_rgb(216, 180, 254)),
                        );
                        ui.add_space(4.0);

                        // Raccourcis Thèmes 1-clic
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Thèmes :").size(10.5));
                            if ui.button("📰 Presse Dorée").clicked() {
                                art_bg_col = "#0f172a".to_string();
                                art_bg_opac = 94;
                                art_border_col = "#f59e0b".to_string();
                                art_border_w = 2;
                                art_title_col = "#fbbf24".to_string();
                                art_text_col = "#f8fafc".to_string();
                                art_badge_col = "#dc2626".to_string();
                                art_glow = true;
                                changed = true;
                            }
                            if ui.button("🚨 Alerte Rouge").clicked() {
                                art_bg_col = "#1c0a0d".to_string();
                                art_bg_opac = 95;
                                art_border_col = "#ef4444".to_string();
                                art_border_w = 3;
                                art_title_col = "#fca5a5".to_string();
                                art_text_col = "#fee2e2".to_string();
                                art_badge_col = "#ef4444".to_string();
                                art_glow = true;
                                changed = true;
                            }
                            if ui.button("💜 Dark VSS").clicked() {
                                art_bg_col = "#15102a".to_string();
                                art_bg_opac = 92;
                                art_border_col = "#a855f7".to_string();
                                art_border_w = 2;
                                art_title_col = "#e9d5ff".to_string();
                                art_text_col = "#f3e8ff".to_string();
                                art_badge_col = "#9333ea".to_string();
                                art_glow = true;
                                changed = true;
                            }
                            if ui.button("📱 Réseau Social").clicked() {
                                art_bg_col = "#0b1220".to_string();
                                art_bg_opac = 92;
                                art_border_col = "#3b82f6".to_string();
                                art_border_w = 2;
                                art_title_col = "#60a5fa".to_string();
                                art_text_col = "#e0f2fe".to_string();
                                art_badge_col = "#2563eb".to_string();
                                art_glow = true;
                                changed = true;
                            }
                            if ui.button("⬛ Transparent").clicked() {
                                art_bg_col = "#05070c".to_string();
                                art_bg_opac = 50;
                                art_border_col = "#475569".to_string();
                                art_border_w = 1;
                                art_title_col = "#ffffff".to_string();
                                art_text_col = "#cbd5e1".to_string();
                                art_badge_col = "#334155".to_string();
                                art_glow = false;
                                changed = true;
                            }
                        });

                        ui.add_space(6.0);

                        // Typographie & Alignement
                        ui.horizontal(|ui| {
                            ui.label("Taille Titre :");
                            if ui.add(egui::Slider::new(&mut art_font_title, 12..=72).suffix(" px")).changed() {
                                changed = true;
                            }
                            ui.label("Taille Texte :");
                            if ui.add(egui::Slider::new(&mut art_font_text, 10..=64).suffix(" px")).changed() {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Alignement :");
                            if ui.selectable_label(art_align == "left", "Gauche").clicked() {
                                art_align = "left".to_string();
                                changed = true;
                            }
                            if ui.selectable_label(art_align == "center", "Centré").clicked() {
                                art_align = "center".to_string();
                                changed = true;
                            }
                            if ui.selectable_label(art_align == "justify", "Justifié").clicked() {
                                art_align = "justify".to_string();
                                changed = true;
                            }
                        });

                        ui.add_space(4.0);

                        // Fond & Bordure
                        ui.horizontal(|ui| {
                            ui.label("Opacité Fond :");
                            if ui.add(egui::Slider::new(&mut art_bg_opac, 0..=100).suffix("%")).changed() {
                                changed = true;
                            }
                            ui.label("Épaisseur Bordure :");
                            if ui.add(egui::Slider::new(&mut art_border_w, 0..=8).suffix(" px")).changed() {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Arrondi Coins :");
                            if ui.add(egui::Slider::new(&mut art_border_rad, 0..=24).suffix(" px")).changed() {
                                changed = true;
                            }
                            if ui.checkbox(&mut art_glow, "✨ Halo Néon (Glow)").changed() {
                                changed = true;
                            }
                        });

                        ui.add_space(4.0);

                        // Défilement automatique
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut art_auto_scroll, "↕️ Défilement Automatique (Auto-Scroll)").changed() {
                                changed = true;
                            }
                            if art_auto_scroll {
                                ui.label("Vitesse :");
                                if ui.add(egui::Slider::new(&mut art_scroll_speed, 1..=8)).changed() {
                                    changed = true;
                                }
                            }
                        });
                    });
            });

        let is_scraping = {
            self.article_scrape_status.lock().map(|s| s.starts_with("⏳")).unwrap_or(false)
        };

        if changed && !is_scraping {
            if let Ok(mut st) = self.state.lock() {
                st.article.visible = art_vis;
                st.article.title = art_title;
                st.article.source = art_source;
                st.article.date = art_date;
                st.article.text = art_text;
                st.article.image_url = art_img;
                st.article.show_image = art_show_img;
                st.article.font_size_title = art_font_title;
                st.article.font_size_text = art_font_text;
                st.article.text_align = art_align;
                st.article.text_color = art_text_col;
                st.article.title_color = art_title_col;
                st.article.source_badge_color = art_badge_col;
                st.article.bg_color = art_bg_col;
                st.article.bg_opacity = art_bg_opac;
                st.article.border_color = art_border_col;
                st.article.border_width = art_border_w;
                st.article.border_radius = art_border_rad;
                st.article.glow = art_glow;
                st.article.auto_scroll = art_auto_scroll;
                st.article.scroll_speed = art_scroll_speed;
                save_state_json(&st);
            }
        }
    }


    // =========================================================
    // 📺 ONGLET DÉDIÉ : YOUTUBE & MÉDIAS EN DIRECT
    // =========================================================
    fn render_tab_youtube(&mut self, ui: &mut egui::Ui) {
        let (vid_act, cur_id, mut auto_p, mut loop_p, mut ctrl_p, mut mute_p, mut anti_d) = {
            let st = self.state.lock().unwrap();
            (
                st.video_active,
                st.video_youtube_id.clone(),
                st.video_yt_autoplay,
                st.video_yt_loop,
                st.video_yt_controls,
                st.video_yt_mute,
                st.video_anti_detect,
            )
        };

        egui::Frame::none()
            .fill(Color32::from_rgb(20, 22, 32))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(239, 68, 68)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📺 LECTEUR VIDÉO YOUTUBE SUR LE LIVE").strong().size(13.5).color(Color32::from_rgb(252, 165, 165)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let st_badge = if vid_act {
                            RichText::new("● EN DIFFUSION LIVE").strong().size(11.0).color(Color32::from_rgb(74, 222, 128))
                        } else {
                            RichText::new("○ EN ATTENTE").size(11.0).color(Color32::from_rgb(156, 163, 175))
                        };
                        ui.label(st_badge);
                    });
                });
                ui.add_space(8.0);

                // Saisie URL YouTube
                ui.label(RichText::new("Collez l'URL ou l'identifiant de la vidéo YouTube :").size(11.0).color(Color32::from_rgb(209, 213, 219)));
                ui.horizontal(|ui| {
                    let text_edit = egui::TextEdit::singleline(&mut self.video_input_yt_url)
                        .hint_text("https://www.youtube.com/watch?v=... ou Shorts...")
                        .desired_width(ui.available_width() - 85.0);
                    let resp = ui.add(text_edit);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let id = extract_youtube_id(&self.video_input_yt_url);
                        if !id.is_empty() {
                            if let Ok(mut st) = self.state.lock() {
                                st.video_source_type = "youtube".to_string();
                                st.video_youtube_id = id;
                                st.video_active = true;
                                st.video_cmd_id += 1;
                                st.video_cmd_action = "play".to_string();
                                save_state_json(&st);
                            }
                        }
                    }

                    if ui.button(RichText::new("▶️ Lancer").strong().color(Color32::WHITE)).clicked() {
                        let id = extract_youtube_id(&self.video_input_yt_url);
                        if !id.is_empty() {
                            if let Ok(mut st) = self.state.lock() {
                                st.video_source_type = "youtube".to_string();
                                st.video_youtube_id = id;
                                st.video_active = true;
                                st.video_cmd_id += 1;
                                st.video_cmd_action = "play".to_string();
                                save_state_json(&st);
                            }
                        }
                    }
                });

                if !cur_id.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("🎬 ID Actif : {}", cur_id)).size(11.0).color(Color32::from_rgb(253, 224, 71)));
                }

                ui.add_space(8.0);
                // Préréglages Rapides
                ui.label(RichText::new("⚡ Préréglages Rapides en 1 Clic :").size(11.0).color(Color32::from_rgb(156, 163, 175)));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("🎙️ Enquête Violences / Témoignage").clicked() {
                        self.video_input_yt_url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string();
                    }
                    if ui.button("📢 Riposte Antifa / Militant").clicked() {
                        self.video_input_yt_url = "https://www.youtube.com/watch?v=eBGIQ7ZuuiU".to_string();
                    }
                    if ui.button("🎵 Musique de Combat").clicked() {
                        self.video_input_yt_url = "https://www.youtube.com/watch?v=ZXsQAXx_ao0".to_string();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                // Options de Lecture & Rendu
                ui.label(RichText::new("⚙️ Options de Présentation & Lecture :").strong().size(11.5).color(Color32::WHITE));
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut auto_p, "Lecture Auto (Autoplay)").changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.video_yt_autoplay = auto_p;
                            save_state_json(&st);
                        }
                    }
                    if ui.checkbox(&mut loop_p, "En Boucle (Loop)").changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.video_yt_loop = loop_p;
                            save_state_json(&st);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut ctrl_p, "Afficher les Contrôles").changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.video_yt_controls = ctrl_p;
                            save_state_json(&st);
                        }
                    }
                    if ui.checkbox(&mut mute_p, "Muet (Mute)").changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.video_yt_mute = mute_p;
                            save_state_json(&st);
                        }
                    }
                });

                if ui.checkbox(&mut anti_d, "🛡️ Filtre Anti-Détection TikTok (Crop & Watermark)").changed() {
                    if let Ok(mut st) = self.state.lock() {
                        st.video_anti_detect = anti_d;
                        save_state_json(&st);
                    }
                }

                ui.add_space(10.0);
                // Commandes de Diffusion
                ui.horizontal(|ui| {
                    let b_toggle = egui::Button::new(
                        RichText::new(if vid_act { "⏹️ ARRÊTER LA VIDÉO" } else { "▶️ ACTIVER LA DIFFUSION" })
                            .strong()
                            .size(12.0)
                            .color(Color32::WHITE),
                    ).fill(if vid_act { Color32::from_rgb(185, 28, 28) } else { Color32::from_rgb(22, 163, 74) });

                    if ui.add_sized([(ui.available_width() - 8.0) / 2.0, 36.0], b_toggle).clicked() {
                        if let Ok(mut st) = self.state.lock() {
                            st.video_active = !st.video_active;
                            if st.video_active {
                                st.video_cmd_id += 1;
                                st.video_cmd_action = "play".to_string();
                            }
                            save_state_json(&st);
                        }
                    }

                    let b_reload = egui::Button::new(
                        RichText::new("🔁 RELANCER VIDÉO").strong().size(12.0).color(Color32::WHITE),
                    ).fill(Color32::from_rgb(79, 70, 229));

                    if ui.add_sized([ui.available_width(), 36.0], b_reload).clicked() {
                        if let Ok(mut st) = self.state.lock() {
                            st.video_active = true;
                            st.video_cmd_id += 1;
                            st.video_cmd_action = "reload".to_string();
                            save_state_json(&st);
                        }
                    }
                });
            });
    }

    // =========================================================
    // 🗳️ ONGLET DÉDIÉ : SONDAGES DU LIVE, JAUGE LIKES & CHAT
    // =========================================================
    fn render_tab_sondages(&mut self, ui: &mut egui::Ui) {
        let (mut poll_state, mut goal_state) = {
            let st = self.state.lock().unwrap();
            (st.poll.clone(), st.like_goal.clone())
        };

        // 1. Module Jauge Objectif Likes / Tapotements (Option B)
        egui::Frame::none()
            .fill(Color32::from_rgb(22, 24, 34))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(239, 68, 68)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🎯 JAUGE OBJECTIF TAPOTEMENTS & LIKES").strong().size(13.0).color(Color32::from_rgb(252, 165, 165)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.checkbox(&mut goal_state.active, "Afficher sur le Live").changed() {
                            if let Ok(mut st) = self.state.lock() {
                                st.like_goal.active = goal_state.active;
                                save_state_json(&st);
                            }
                        }
                    });
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Titre :").size(11.0));
                    if ui.text_edit_singleline(&mut goal_state.title).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.like_goal.title = goal_state.title.clone();
                            save_state_json(&st);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Likes Actuels :").size(11.0));
                    let mut cur_likes_i32 = goal_state.current_likes as i32;
                    if ui.add(egui::DragValue::new(&mut cur_likes_i32).speed(50.0).range(0..=1_000_000)).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.like_goal.current_likes = cur_likes_i32.max(0) as u64;
                            save_state_json(&st);
                        }
                    }

                    ui.label(RichText::new("Objectif :").size(11.0));
                    let mut tgt_likes_i32 = goal_state.target_likes as i32;
                    if ui.add(egui::DragValue::new(&mut tgt_likes_i32).speed(100.0).range(100..=5_000_000)).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.like_goal.target_likes = tgt_likes_i32.max(10) as u64;
                            save_state_json(&st);
                        }
                    }
                });

                ui.add_space(4.0);
                let pct = ((goal_state.current_likes as f32 / goal_state.target_likes.max(1) as f32) * 100.0).clamp(0.0, 100.0);
                ui.add(egui::ProgressBar::new(pct / 100.0).text(format!("{:.1}% ({} / {})", pct, goal_state.current_likes, goal_state.target_likes)));
            });

        ui.add_space(8.0);

        // 2. Module Sondage Interactif du Live
        egui::Frame::none()
            .fill(Color32::from_rgb(20, 24, 38))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(59, 130, 246)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🗳️ SONDAGE EN DIRECT DU CHAT (Tapez !1, !2...)").strong().size(13.0).color(Color32::from_rgb(147, 197, 253)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.checkbox(&mut poll_state.active, "Sondage Ouvert").changed() {
                            if let Ok(mut st) = self.state.lock() {
                                st.poll.active = poll_state.active;
                                st.song_vote.active = poll_state.active;
                                st.song_vote.title = poll_state.question.clone();
                                save_state_json(&st);
                            }
                        }
                    });
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Question :").size(11.0));
                    if ui.text_edit_singleline(&mut poll_state.question).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.poll.question = poll_state.question.clone();
                            st.song_vote.title = poll_state.question.clone();
                            save_state_json(&st);
                        }
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("!1 :");
                    if ui.text_edit_singleline(&mut poll_state.choice1).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.poll.choice1 = poll_state.choice1.clone();
                            st.song_vote.choice1 = poll_state.choice1.clone();
                            save_state_json(&st);
                        }
                    }
                    ui.label("!2 :");
                    if ui.text_edit_singleline(&mut poll_state.choice2).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.poll.choice2 = poll_state.choice2.clone();
                            st.song_vote.choice2 = poll_state.choice2.clone();
                            save_state_json(&st);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("!3 :");
                    if ui.text_edit_singleline(&mut poll_state.choice3).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.poll.choice3 = poll_state.choice3.clone();
                            st.song_vote.choice3 = poll_state.choice3.clone();
                            save_state_json(&st);
                        }
                    }
                    ui.label("!4 :");
                    if ui.text_edit_singleline(&mut poll_state.choice4).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.poll.choice4 = poll_state.choice4.clone();
                            st.song_vote.choice4 = poll_state.choice4.clone();
                            save_state_json(&st);
                        }
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("🔄 Réinitialiser les Votes").size(11.0)).clicked() {
                        if let Ok(mut st) = self.state.lock() {
                            st.poll.votes1 = 0;
                            st.poll.votes2 = 0;
                            st.poll.votes3 = 0;
                            st.poll.votes4 = 0;
                            st.poll.voters.clear();
                            st.song_vote.votes1 = 0;
                            st.song_vote.votes2 = 0;
                            st.song_vote.votes3 = 0;
                            st.song_vote.votes4 = 0;
                            st.song_vote.voters.clear();
                            save_state_json(&st);
                        }
                    }
                });
            });

        ui.add_space(8.0);
        self.render_tab_chat(ui);
    }

    // =========================================================
    // ⚙️ ONGLET DÉDIÉ : PRÉFÉRENCES, 8 DÉCORS & AUTO-TAPOTEUR
    // =========================================================
    fn render_tab_preferences(&mut self, ui: &mut egui::Ui) {
        let (mut host_acc, mut helper_acc, mut tapper_cfg, bgs) = {
            let st = self.state.lock().unwrap();
            (
                st.tiktok_host_account.clone(),
                st.tiktok_helper_account.clone(),
                st.tapper_config.clone(),
                st.custom_backgrounds.clone(),
            )
        };

        // 1. Identité TikTok & Comptes
        egui::Frame::none()
            .fill(Color32::from_rgb(20, 22, 30))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(168, 85, 247)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(RichText::new("👤 COMPTES TIKTOK & DIFFUSION").strong().size(13.0).color(Color32::from_rgb(216, 180, 254)));
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Compte Hôte du Live :").size(11.5));
                    if ui.text_edit_singleline(&mut host_acc).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.tiktok_host_account = host_acc.clone();
                            st.chat_handle = host_acc.clone();
                            save_state_json(&st);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Compte Secondaire (Tapoteur) :").size(11.5));
                    if ui.text_edit_singleline(&mut helper_acc).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.tiktok_helper_account = helper_acc.clone();
                            save_state_json(&st);
                        }
                    }
                });
            });

        ui.add_space(8.0);

        // 2. Gestionnaire des 8 Décors / Images de Fond (Sans Recompilation)
        egui::Frame::none()
            .fill(Color32::from_rgb(22, 25, 36))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(59, 130, 246)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🖼️ GESTIONNAIRE DES 8 DÉCORS (IMAGES DE FOND)").strong().size(13.0).color(Color32::from_rgb(147, 197, 253)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🔄 Décors par Défaut").clicked() {
                            if let Ok(mut st) = self.state.lock() {
                                st.custom_backgrounds = CustomBackgrounds::default();
                                save_state_json(&st);
                            }
                        }
                    });
                });
                ui.label(RichText::new("Modifiez vos 8 décors en 1 clic sans jamais recompiler l'application.").size(10.5).color(Color32::from_rgb(156, 163, 175)));
                ui.add_space(8.0);

                // Section 9:16 Vertical
                ui.label(RichText::new("📱 4 DÉCORS VERTICAUX (9:16 - 1080x1920) :").strong().size(11.5).color(Color32::WHITE));
                ui.add_space(4.0);

                Self::render_bg_slot(ui, &self.state, "vss_9_16", "💜 VSS & Écoute (9:16)", &bgs.vss_9_16);
                Self::render_bg_slot(ui, &self.state, "politique_9_16", "✊ Débat Politique (9:16)", &bgs.politique_9_16);
                Self::render_bg_slot(ui, &self.state, "musique_9_16", "🎸 Musique & Sons (9:16)", &bgs.musique_9_16);
                Self::render_bg_slot(ui, &self.state, "chill_9_16", "☕ Chill & Détente (9:16)", &bgs.chill_9_16);

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Section 16:9 Horizontal
                ui.label(RichText::new("🖥️ 4 DÉCORS PAYSAGE (16:9 - 1920x1080) :").strong().size(11.5).color(Color32::WHITE));
                ui.add_space(4.0);

                Self::render_bg_slot(ui, &self.state, "vss_16_9", "💜 VSS & Écoute (16:9)", &bgs.vss_16_9);
                Self::render_bg_slot(ui, &self.state, "politique_16_9", "✊ Débat Politique (16:9)", &bgs.politique_16_9);
                Self::render_bg_slot(ui, &self.state, "musique_16_9", "🎸 Musique & Sons (16:9)", &bgs.musique_16_9);
                Self::render_bg_slot(ui, &self.state, "chill_16_9", "☕ Chill & Détente (16:9)", &bgs.chill_16_9);
            });

        ui.add_space(8.0);

        // 3. Module Auto-Tapoteur Anti-Détection (Option 1 & 2)
        egui::Frame::none()
            .fill(Color32::from_rgb(26, 20, 24))
            .stroke(Stroke::new(1.2_f32, Color32::from_rgb(244, 63, 94)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("👆 AUTO-TAPOTEUR INTELLIGENT (ANTI-DÉTECTION)").strong().size(13.0).color(Color32::from_rgb(253, 164, 175)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let badge_col = if tapper_cfg.enabled { Color32::from_rgb(74, 222, 128) } else { Color32::from_rgb(156, 163, 175) };
                        let badge_txt = if tapper_cfg.enabled { "● TAPOTAGE ACTIF" } else { "○ INACTIF" };
                        ui.label(RichText::new(badge_txt).strong().size(11.0).color(badge_col));
                    });
                });
                ui.label(RichText::new("Tapote en continu sans monopoliser votre curseur physique de souris.").size(10.5).color(Color32::from_rgb(209, 213, 219)));
                ui.add_space(8.0);

                // Choix du mode
                ui.label(RichText::new("Méthode de tapotage :").size(11.0));
                ui.horizontal(|ui| {
                    if ui.selectable_label(tapper_cfg.mode == "post_message", "Option 1: Arrière-Plan Windows (PostMessage)").clicked() {
                        tapper_cfg.mode = "post_message".to_string();
                        if let Ok(mut st) = self.state.lock() {
                            st.tapper_config.mode = tapper_cfg.mode.clone();
                            save_state_json(&st);
                        }
                    }
                    if ui.selectable_label(tapper_cfg.mode == "webview", "Option 2: Mini-Session Web / Virtuelle").clicked() {
                        tapper_cfg.mode = "webview".to_string();
                        if let Ok(mut st) = self.state.lock() {
                            st.tapper_config.mode = tapper_cfg.mode.clone();
                            save_state_json(&st);
                        }
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Titre de la fenêtre TikTok à cibler :").size(11.0));
                    if ui.text_edit_singleline(&mut tapper_cfg.target_window_title).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.tapper_config.target_window_title = tapper_cfg.target_window_title.clone();
                            save_state_json(&st);
                        }
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Cadence :").size(11.0));
                    let mut cps_i32 = tapper_cfg.clicks_per_second as i32;
                    if ui.add(egui::Slider::new(&mut cps_i32, 1..=12).text("tapotements / sec")).changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.tapper_config.clicks_per_second = cps_i32.clamp(1, 15) as u32;
                            save_state_json(&st);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut tapper_cfg.jitter_enabled, "Dispersion aléatoire ±5px (Imitation humaine)").changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.tapper_config.jitter_enabled = tapper_cfg.jitter_enabled;
                            save_state_json(&st);
                        }
                    }
                    if ui.checkbox(&mut tapper_cfg.random_pauses, "Micro-pauses naturelles anti-ban").changed() {
                        if let Ok(mut st) = self.state.lock() {
                            st.tapper_config.random_pauses = tapper_cfg.random_pauses;
                            save_state_json(&st);
                        }
                    }
                });

                ui.add_space(10.0);
                let btn_tapper = egui::Button::new(
                    RichText::new(if tapper_cfg.enabled { "⏹️ ARRÊTER LE TAPOTAGE" } else { "▶️ DÉMARRER LE TAPOTAGE (ARRIÈRE-PLAN)" })
                        .strong()
                        .size(12.5)
                        .color(Color32::WHITE),
                ).fill(if tapper_cfg.enabled { Color32::from_rgb(225, 29, 72) } else { Color32::from_rgb(16, 185, 129) });

                if ui.add_sized([ui.available_width(), 38.0], btn_tapper).clicked() {
                    if let Ok(mut st) = self.state.lock() {
                        st.tapper_config.enabled = !st.tapper_config.enabled;
                        save_state_json(&st);
                    }
                }
            });
    }

    fn render_bg_slot(ui: &mut egui::Ui, state: &Arc<Mutex<FullLiveState>>, slot_name: &str, label: &str, current_path: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(11.0).color(Color32::from_rgb(229, 231, 235)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("📂 Parcourir...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Images (JPG, PNG, WEBP)", &["jpg", "jpeg", "png", "webp"])
                        .pick_file() {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                        let dest_name = format!("custom_bg_{}.{}", slot_name, ext);
                        let dest_full = app_path("assets").join(&dest_name);
                        if let Ok(_) = std::fs::copy(&path, &dest_full) {
                            let url_with_v = format!("/assets/{}?v={}", dest_name, now_secs() as u64);
                            if let Ok(mut st) = state.lock() {
                                match slot_name {
                                    "vss_9_16" => st.custom_backgrounds.vss_9_16 = url_with_v,
                                    "politique_9_16" => st.custom_backgrounds.politique_9_16 = url_with_v,
                                    "musique_9_16" => st.custom_backgrounds.musique_9_16 = url_with_v,
                                    "chill_9_16" => st.custom_backgrounds.chill_9_16 = url_with_v,
                                    "vss_16_9" => st.custom_backgrounds.vss_16_9 = url_with_v,
                                    "politique_16_9" => st.custom_backgrounds.politique_16_9 = url_with_v,
                                    "musique_16_9" => st.custom_backgrounds.musique_16_9 = url_with_v,
                                    "chill_16_9" => st.custom_backgrounds.chill_16_9 = url_with_v,
                                    _ => {}
                                }
                                save_state_json(&st);
                            }
                        }
                    }
                }
                let short = if current_path.len() > 30 {
                    format!("...{}", &current_path[current_path.len() - 27..])
                } else {
                    current_path.to_string()
                };
                ui.label(RichText::new(short).size(10.0).color(Color32::from_rgb(156, 163, 175)));
            });
        });
    }

    fn render_tab_soundboard(&mut self, ui: &mut egui::Ui) {
                // =========================================================
                // 5. SOUNDBOARD / JINGLES INSTANTANÉS (MP3 & VIDÉOS)
                // =========================================================
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(24, 26, 34))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(168, 85, 247)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        let mut add_jingle_clicked = false;
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("🔊 5. BOÎTE À JINGLES & MÉDIAS DU LIVE (MP3 & VIDÉOS)")
                                    .strong()
                                    .color(Color32::from_rgb(216, 180, 254)),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("➕ Ajouter Jingle (MP3 / Vidéo)...")
                                                .strong()
                                                .size(11.5)
                                                .color(Color32::WHITE),
                                        )
                                        .fill(Color32::from_rgb(124, 58, 237)),
                                    )
                                    .clicked()
                                {
                                    add_jingle_clicked = true;
                                }
                            });
                        });
                        ui.add_space(4.0);

                        if add_jingle_clicked {
                            if let Some(file) = rfd::FileDialog::new()
                                .add_filter("Médias Jingles (MP3, WAV, MP4, WebM)", &["mp3", "wav", "ogg", "m4a", "mp4", "webm", "mov"])
                                .pick_file()
                            {
                                let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                                let is_vid = ["mp4", "webm", "mov"].contains(&ext.as_str());
                                let label_raw = file.file_stem().and_then(|s| s.to_str()).unwrap_or("Jingle").to_string();
                                let path_str = file.to_string_lossy().to_string();
                                let id = format!("custom_{}", now_secs() as u64);
                                let prefix = if is_vid { "🎬 " } else { "🎵 " };
                                if let Ok(mut st) = self.state.lock() {
                                    st.custom_jingles.push(JingleItem {
                                        id,
                                        label: format!("{}{}", prefix, label_raw),
                                        file_path: path_str,
                                        media_type: if is_vid { "video".into() } else { "audio".into() },
                                        volume: 80,
                                    });
                                    save_state_json(&st);
                                }
                                self.save_jingles();
                            }
                        }

                        let (mut jingles_copy, mut to_delete, mut to_trigger) = {
                            let st = self.state.lock().unwrap();
                            (st.custom_jingles.clone(), None, None)
                        };

                        let mut vol_changed = false;
                        egui::ScrollArea::vertical()
                            .id_salt("jingles_scroll_area")
                            .max_height(160.0)
                            .show(ui, |ui| {
                                for (idx, jingle) in jingles_copy.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        let btn_col = match jingle.media_type.as_str() {
                                            "video" => Color32::from_rgb(124, 58, 237),
                                            "audio" => Color32::from_rgb(13, 148, 136),
                                            _ => match idx % 5 {
                                                0 => Color32::from_rgb(220, 38, 38),
                                                1 => Color32::from_rgb(147, 51, 234),
                                                2 => Color32::from_rgb(37, 99, 235),
                                                3 => Color32::from_rgb(75, 85, 99),
                                                _ => Color32::from_rgb(217, 119, 6),
                                            },
                                        };

                                        if ui
                                            .add_sized(
                                                [130.0, 26.0],
                                                egui::Button::new(
                                                    RichText::new(&jingle.label)
                                                        .strong()
                                                        .size(11.5)
                                                        .color(Color32::WHITE),
                                                )
                                                .fill(btn_col),
                                            )
                                            .clicked()
                                        {
                                            to_trigger = Some(jingle.id.clone());
                                        }

                                        ui.label(RichText::new("Vol:").size(11.0).color(Color32::from_rgb(203, 213, 225)));
                                        if ui.add(egui::Slider::new(&mut jingle.volume, 0..=100).suffix("%").show_value(true)).changed() {
                                            vol_changed = true;
                                        }

                                        if jingle.media_type != "synth" {
                                            if ui.small_button("🗑️").on_hover_text("Supprimer ce jingle").clicked() {
                                                to_delete = Some(idx);
                                            }
                                        }
                                    });
                                    ui.add_space(2.0);
                                }
                            });

                        if vol_changed {
                            if let Ok(mut st) = self.state.lock() {
                                st.custom_jingles = jingles_copy;
                                save_state_json(&st);
                            }
                            self.save_jingles();
                        }
                        if let Some(del_idx) = to_delete {
                            if let Ok(mut st) = self.state.lock() {
                                if del_idx < st.custom_jingles.len() {
                                    st.custom_jingles.remove(del_idx);
                                    save_state_json(&st);
                                }
                            }
                            self.save_jingles();
                        }
                        if let Some(item_id) = to_trigger {
                            self.trigger_jingle_item(&item_id);
                        }
                    });

                ui.add_space(8.0);

    }
}

impl eframe::App for TelecommandeApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_tiktok_chat_bridge();
        flush_state_json();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Appliquer un thème sombre haut contraste Dark Fantasy absolu
        // Empêche tout widget, champ texte ou scrollbox de devenir blanc ou illisible
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(Color32::from_rgb(241, 245, 249));
        visuals.window_fill = Color32::from_rgb(15, 17, 24);
        visuals.panel_fill = Color32::from_rgb(15, 17, 24);
        visuals.extreme_bg_color = Color32::from_rgb(22, 26, 38);
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(24, 28, 40);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 36, 52);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 54, 78);
        visuals.widgets.active.bg_fill = Color32::from_rgb(59, 130, 246);
        ctx.set_visuals(visuals);

        // Fréquence de rafraîchissement adaptative :
        // 60ms (~16 FPS) si de l'audio est détecté pour animer la barre de volume sans à-coup,
        // sinon 250ms (~4 FPS) au repos pour soulager à 100% le processeur et la carte graphique
        let peak = f32::from_bits(self.audio_peak_atomic.load(std::sync::atomic::Ordering::Relaxed));
        if peak > 0.005 {
            ctx.request_repaint_after(Duration::from_millis(60));
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        // Afficher la fenêtre dédiée de configuration des numéros d'urgence si ouverte
        self.render_emergency_config_viewport(ctx);
        // Afficher la fenêtre dédiée de configuration du Bot TikTok si ouverte
        self.render_bot_config_viewport(ctx);
        // Les raccourcis globaux F1–F4 / F8–F12 sont gérés par un thread dédié (voir main)


        let panel_frame = egui::Frame::none()
            .fill(Color32::from_rgb(16, 17, 22))
            .inner_margin(egui::Margin::same(8.0));

        egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
            self.render_shield_bar(ui);
            if self.mini_mode {
                self.render_mini_dashboard(ui, ctx);
            } else {
                self.render_top_quick_bar(ui, ctx);
                ui.add_space(5.0);
                self.render_tab_selector(ui);
                ui.add_space(5.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        match self.active_tab {
                            TelecommandeTab::Regie => self.render_tab_regie(ui),
                            TelecommandeTab::Youtube => self.render_tab_youtube(ui),
                            TelecommandeTab::Articles => self.render_tab_articles(ui),
                            TelecommandeTab::Sondages => self.render_tab_sondages(ui),
                            TelecommandeTab::Soundboard => self.render_tab_soundboard(ui),
                            TelecommandeTab::Preferences => self.render_tab_preferences(ui),
                            TelecommandeTab::Chat => self.render_tab_chat(ui),
                            TelecommandeTab::Bannieres => self.render_tab_bannieres(ui),
                        }
                    });
            }
        });
    }
}
fn run_http_server(state: Arc<Mutex<FullLiveState>>) {
    let mut server = None;
    for attempt in 1..=10 {
        match Server::http("127.0.0.1:8765") {
            Ok(s) => {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
                    let _ = writeln!(f, "SERVEUR HTTP: Ecoute active sur 127.0.0.1:8765 (tentative {})", attempt);
                }
                server = Some(s);
                break;
            }
            Err(e) => {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
                    let _ = writeln!(f, "SERVEUR HTTP tentative {}: erreur bind 8765: {:?}", attempt, e);
                }
                thread::sleep(Duration::from_millis(300));
            }
        }
    }

    let server = match server {
        Some(s) => Arc::new(s),
        None => return,
    };

    // 8 threads workers HTTP parallèles : le streaming vidéo MP4 ou une requête lente ne bloque JAMAIS la télécommande !
    for _ in 0..8 {
        let srv = Arc::clone(&server);
        let state = Arc::clone(&state);
        thread::spawn(move || {
            for req in srv.incoming_requests() {
                handle_http_request(req, &state);
            }
        });
    }
}

fn resolve_app_file(rel_path: &str) -> Option<Vec<u8>> {
    std::fs::read(app_path(rel_path)).ok()
}

fn handle_http_request(mut req: tiny_http::Request, state: &Arc<Mutex<FullLiveState>>) {
    let url = req.url().to_string();
    let cors = Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
    let no_cache = Header::from_bytes(&b"Cache-Control"[..], &b"no-store, no-cache, must-revalidate, max-age=0"[..]).unwrap();
    let conn_close = Header::from_bytes(&b"Connection"[..], &b"close"[..]).unwrap();

    if url.starts_with("/chat_ingest") && req.method() == &tiny_http::Method::Post {
            let mut body = Vec::new();
            let _ = req.as_reader().read_to_end(&mut body);
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&body) {
                if let Ok(mut st) = state.lock() {
                    let ev_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if ev_type == "status" {
                        if let Some(s) = val.get("status").and_then(|v| v.as_str()) {
                            st.chat_status = s.to_string();
                        }
                    } else if ev_type == "gift" || ev_type == "share" || ev_type == "follow" {
                        let count = val.get("count").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as u32;
                        match ev_type {
                            "gift" => st.journal.gifts += count as u64,
                            "share" => st.journal.shares += 1,
                            _ => st.journal.follows += 1,
                        }
                        if st.thanks_enabled && !st.shield_active {
                            let author = val.get("author").and_then(|v| v.as_str()).unwrap_or("Spectateur").to_string();
                            let gift = val.get("gift").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            push_thanks(&mut st, ev_type, author, gift, count);
                        }
                    } else if ev_type == "comment" {
                        let author = val.get("author").and_then(|v| v.as_str()).unwrap_or("Spectateur").to_string();
                        let handle = val.get("handle").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let text = val.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let avatar = val.get("avatar").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let is_question = text.contains('?');
                        let shield = st.shield_active;
                        st.journal.comments += 1;

                        let lower = text.to_lowercase();

                        // 0. Demande d'aide (mots-clés des déclencheurs "emergency") : toujours notée dans la file
                        //    privée de la régie, AVANT le filtre anti-insultes (une personne en détresse peut jurer)
                        let asks_help = st.bot_config.triggers.iter().any(|trig| {
                            trig.enabled
                                && trig.action == "emergency"
                                && trig
                                    .keywords
                                    .split(',')
                                    .map(|w| w.trim().to_lowercase())
                                    .any(|kw| !kw.is_empty() && lower.contains(&kw))
                        });
                        if asks_help {
                            st.help_alert_seq += 1;
                            let id = st.help_alert_seq;
                            st.help_alerts.push(HelpAlert {
                                id,
                                ts: now_secs(),
                                time_label: local_time_label(),
                                author: author.clone(),
                                handle: handle.clone(),
                                text: text.clone(),
                                handled: false,
                            });
                            if st.help_alerts.len() > 200 {
                                st.help_alerts.remove(0);
                            }
                        }

                        // 1. Filtre anti-insultes
                        if st.bot_config.filter_blacklist {
                            let blacklist: Vec<&str> = st
                                .bot_config
                                .blacklist_words
                                .split(',')
                                .map(|w| w.trim())
                                .filter(|w| !w.is_empty())
                                .collect();
                            if blacklist.iter().any(|bad| lower.contains(bad)) {
                                if asks_help {
                                    // Message masqué du chat, mais la demande d'aide reste prise en compte
                                    st.emergency_visible = true;
                                    save_state_json(&st);
                                }
                                // Message filtré silencieusement, non affiché
                                let _ = req.respond(Response::from_string("filtered").with_header(cors).with_header(conn_close));
                                return;
                            }
                        }

                        // 2. Déclencheurs et Règles personnalisées du Bot
                        let triggers_copy = st.bot_config.triggers.clone();
                        let auto_pin_q = st.bot_config.auto_pin_questions;
                        let mut is_help = false;

                        for trig in &triggers_copy {
                            if !trig.enabled {
                                continue;
                            }
                            let keywords: Vec<String> = trig
                                .keywords
                                .split(',')
                                .map(|w| w.trim().to_lowercase())
                                .filter(|w| !w.is_empty())
                                .collect();

                            if keywords.iter().any(|kw| lower.contains(kw)) {
                                match trig.action.as_str() {
                                    "emergency" => {
                                        is_help = true;
                                        st.emergency_visible = true;
                                    }
                                    // Bouclier actif : le chat ne peut plus déclencher de jingle ni épingler (l'urgence reste active)
                                    "jingle" if !shield => {
                                        st.jingle_cmd_id += 1;
                                        st.jingle_type = if trig.action_param.is_empty() {
                                            "alerte".to_string()
                                        } else {
                                            trig.action_param.clone()
                                        };
                                    }
                                    "pin" if !shield => {
                                        st.pinned_author = author.clone();
                                        st.pinned_text = text.clone();
                                        st.pinned_avatar = avatar.clone();
                                        st.pinned_visible = true;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        // 3. Jukebox & Votes Musicaux du Chat
                        let trimmed = lower.trim();
                        if st.song_mode == "queue" {
                            if trimmed.starts_with("!musique ") || trimmed.starts_with("!sr ") || trimmed.starts_with("!song ") {
                                let parts: Vec<&str> = text.splitn(2, ' ').collect();
                                if parts.len() == 2 {
                                    let query = parts[1].trim().to_string();
                                    if !query.is_empty() {
                                        // Anti-spam : 1 demande max par auteur dans la file
                                        let already_in_queue = st.song_requests.iter().any(|r| {
                                            r.author.eq_ignore_ascii_case(&author)
                                                || (!handle.is_empty() && r.handle.eq_ignore_ascii_case(&handle))
                                        });
                                        if !already_in_queue {
                                            let next_id = st.song_requests.len() as u64 + 1;
                                            st.song_requests.push(SongRequestItem {
                                                id: next_id,
                                                author: author.clone(),
                                                handle: handle.clone(),
                                                query,
                                                timestamp: now_secs(),
                                            });
                                            if st.song_requests.len() > 40 {
                                                st.song_requests.remove(0);
                                            }
                                        }
                                    }
                                }
                            }
                        } else if st.song_mode == "vote" && st.song_vote.active {
                            let voter_id = if !handle.is_empty() { handle.clone() } else { author.clone() };
                            if !st.song_vote.voters.contains(&voter_id) {
                                let mut voted = false;
                                if trimmed == "!1" || trimmed == "1" {
                                    st.song_vote.votes1 += 1;
                                    voted = true;
                                } else if trimmed == "!2" || trimmed == "2" {
                                    st.song_vote.votes2 += 1;
                                    voted = true;
                                } else if trimmed == "!3" || trimmed == "3" {
                                    st.song_vote.votes3 += 1;
                                    voted = true;
                                } else if trimmed == "!4" || trimmed == "4" {
                                    st.song_vote.votes4 += 1;
                                    voted = true;
                                }
                                if voted {
                                    st.song_vote.voters.push(voter_id);
                                }
                            }
                        }

                        if is_question && auto_pin_q && !shield {
                            st.pinned_author = author.clone();
                            st.pinned_text = text.clone();
                            st.pinned_avatar = avatar.clone();
                            st.pinned_visible = true;
                        }

                        st.chat_comments.push(ChatCommentItem {
                            author,
                            handle,
                            text,
                            avatar,
                            is_question,
                            is_help_trigger: is_help,
                        });
                        if st.chat_comments.len() > 100 {
                            st.chat_comments.remove(0);
                        }
                    }
                    save_state_json(&st);
                }
            }
            let _ = req.respond(Response::from_string("ok").with_header(cors).with_header(conn_close));
            return;
        }

        if url.starts_with("/state") {
            let json_bytes = {
                let mut st = state.lock().unwrap();
                // La file des demandes d'aide reste privée à la régie : retirée le temps de sérialiser
                let alerts = std::mem::take(&mut st.help_alerts);
                let journal = std::mem::take(&mut st.journal);
                let bytes = serde_json::to_vec(&*st).unwrap_or_default();
                st.help_alerts = alerts;
                st.journal = journal;
                bytes
            };
            let ct = Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..]).unwrap();
            let _ = req.respond(
                Response::from_data(json_bytes)
                    .with_header(ct)
                    .with_header(cors)
                    .with_header(no_cache)
                    .with_header(conn_close),
            );
        } else if url.starts_with("/stage_16_9") {
            let html = resolve_app_file("stage_16_9.html").unwrap_or_else(|| b"Stage 16:9 HTML missing".to_vec());
            let ct = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let _ = req.respond(
                Response::from_data(html)
                    .with_header(ct)
                    .with_header(cors)
                    .with_header(no_cache)
                    .with_header(conn_close),
            );
        } else if url.starts_with("/stage") {
            let html = resolve_app_file("stage_solution_b.html").unwrap_or_else(|| b"Stage HTML missing".to_vec());
            let ct = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let _ = req.respond(
                Response::from_data(html)
                    .with_header(ct)
                    .with_header(cors)
                    .with_header(no_cache)
                    .with_header(conn_close),
            );
        } else if url.starts_with("/article") {
            let html = resolve_app_file("article_window.html").unwrap_or_else(|| b"Article HTML missing".to_vec());
            let ct = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let _ = req.respond(
                Response::from_data(html)
                    .with_header(ct)
                    .with_header(cors)
                    .with_header(no_cache)
                    .with_header(conn_close),
            );
        } else if url.starts_with("/urgences") {
            let html = resolve_app_file("urgences_window.html").unwrap_or_else(|| b"Urgences HTML missing".to_vec());
            let ct = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
            let _ = req.respond(
                Response::from_data(html)
                    .with_header(ct)
                    .with_header(cors)
                    .with_header(no_cache)
                    .with_header(conn_close),
            );
        } else if url.starts_with("/assets/") {
            let clean_path = url.split('?').next().unwrap_or("").trim_start_matches('/');
            // Refuse toute sortie du dossier assets/ (../, chemins Windows absolus ou antislash)
            let is_safe = !clean_path.contains("..") && !clean_path.contains('\\') && !clean_path.contains(':');
            if let Some(bytes) = if is_safe { resolve_app_file(clean_path) } else { None } {
                let lower = clean_path.to_lowercase();
                let mime = if lower.ends_with(".png") {
                    &b"image/png"[..]
                } else if lower.ends_with(".webp") {
                    &b"image/webp"[..]
                } else if lower.ends_with(".gif") {
                    &b"image/gif"[..]
                } else {
                    &b"image/jpeg"[..]
                };
                let ct = Header::from_bytes(&b"Content-Type"[..], mime).unwrap();
                let _ = req.respond(
                    Response::from_data(bytes)
                        .with_header(ct)
                        .with_header(cors)
                        .with_header(no_cache)
                        .with_header(conn_close),
                );
            } else {
                let _ = req.respond(Response::empty(404).with_header(conn_close));
            }
        } else if url.starts_with("/video_stream") {
            let video_path = {
                let st = state.lock().unwrap();
                st.video_file_path.clone()
            };
            let path_buf = PathBuf::from(&video_path);
            if let Ok(mut file) = File::open(&path_buf) {
                let total_len = file.metadata().map(|m| m.len()).unwrap_or(0);
                let range_header = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Range"))
                    .map(|h| h.value.as_str().to_string());

                if let Some(r_str) = range_header {
                    let clean = r_str.trim_start_matches("bytes=");
                    let mut parts = clean.split('-');
                    let start: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let end: u64 = parts
                        .next()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(|| (start + 2_097_152).min(total_len.saturating_sub(1)));
                    let chunk_len = (end.saturating_sub(start) + 1).min(4_194_304) as usize;

                    let _ = file.seek(SeekFrom::Start(start));
                    let mut buf = vec![0u8; chunk_len];
                    let n = file.read(&mut buf).unwrap_or(0);
                    buf.truncate(n);

                    let cr = Header::from_bytes(
                        &b"Content-Range"[..],
                        format!("bytes {}-{}/{}", start, start + n.saturating_sub(1) as u64, total_len).as_bytes(),
                    )
                    .unwrap();
                    let ar = Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap();
                    let ct = Header::from_bytes(&b"Content-Type"[..], &b"video/mp4"[..]).unwrap();
                    let _ = req.respond(
                        Response::from_data(buf)
                            .with_status_code(206)
                            .with_header(cr)
                            .with_header(ar)
                            .with_header(ct)
                            .with_header(cors)
                            .with_header(conn_close),
                    );
                } else {
                    // Si le navigateur ne demande pas de Range, on renvoie quand même un premier bloc 206 Partial Content de 2 Mo
                    // pour ne jamais bloquer un thread pendant 30 secondes sur un gros fichier vidéo !
                    let chunk_len = 2_097_152usize.min(total_len as usize);
                    let mut buf = vec![0u8; chunk_len];
                    let n = file.read(&mut buf).unwrap_or(0);
                    buf.truncate(n);
                    let cr = Header::from_bytes(
                        &b"Content-Range"[..],
                        format!("bytes 0-{}/{}", n.saturating_sub(1), total_len).as_bytes(),
                    )
                    .unwrap();
                    let ar = Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap();
                    let ct = Header::from_bytes(&b"Content-Type"[..], &b"video/mp4"[..]).unwrap();
                    let _ = req.respond(
                        Response::from_data(buf)
                            .with_status_code(206)
                            .with_header(cr)
                            .with_header(ar)
                            .with_header(ct)
                            .with_header(cors)
                            .with_header(conn_close),
                    );
                }
            } else {
                let _ = req.respond(Response::empty(404).with_header(conn_close));
            }
        } else if url.starts_with("/jingle_stream") {
            let jingle_path = {
                let st = state.lock().unwrap();
                st.jingle_file_path.clone()
            };
            let path_buf = PathBuf::from(&jingle_path);
            if let Ok(bytes) = std::fs::read(&path_buf) {
                let lower = jingle_path.to_lowercase();
                let mime = if lower.ends_with(".mp4") {
                    &b"video/mp4"[..]
                } else if lower.ends_with(".webm") {
                    &b"video/webm"[..]
                } else if lower.ends_with(".wav") {
                    &b"audio/wav"[..]
                } else if lower.ends_with(".ogg") {
                    &b"audio/ogg"[..]
                } else {
                    &b"audio/mpeg"[..]
                };
                let ct = Header::from_bytes(&b"Content-Type"[..], mime).unwrap();
                let _ = req.respond(
                    Response::from_data(bytes)
                        .with_header(ct)
                        .with_header(cors)
                        .with_header(no_cache)
                        .with_header(conn_close),
                );
            } else {
                let _ = req.respond(Response::empty(404).with_header(conn_close));
            }
        } else {
            let _ = req.respond(Response::empty(404).with_header(conn_close));
        }
}

fn bring_existing_window_to_front() -> bool {
    unsafe {
        use windows::core::HSTRING;
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};
        let title = HSTRING::from("🎛️ TELECOMMANDE_RUST — Briser le Silence");
        if let Ok(hwnd) = FindWindowW(None, &title) {
            if !hwnd.is_invalid() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
                return true;
            }
        }
    }
    false
}


unsafe fn find_window_by_substring(needle: &str) -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, IsWindowVisible};

    struct SearchCtx<'a> {
        needle: &'a str,
        found: Option<HWND>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut SearchCtx);
        if IsWindowVisible(hwnd).as_bool() {
            let mut title = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut title);
            if len > 0 {
                let text = String::from_utf16_lossy(&title[..len as usize]).to_lowercase();
                if text.contains(&ctx.needle.to_lowercase()) {
                    ctx.found = Some(hwnd);
                    return BOOL(0);
                }
            }
        }
        BOOL(1)
    }

    let mut ctx = SearchCtx { needle, found: None };
    let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    ctx.found
}

fn run_background_tapper(state: Arc<Mutex<FullLiveState>>) {
    thread::spawn(move || {
        let mut click_count: u64 = 0;
        loop {
            let (enabled, mode, cps, jitter, pauses, target_title) = {
                if let Ok(st) = state.lock() {
                    (
                        st.tapper_config.enabled,
                        st.tapper_config.mode.clone(),
                        st.tapper_config.clicks_per_second.clamp(1, 15),
                        st.tapper_config.jitter_enabled,
                        st.tapper_config.random_pauses,
                        st.tapper_config.target_window_title.clone(),
                    )
                } else {
                    (false, "post_message".to_string(), 4, true, true, "TikTok".to_string())
                }
            };

            // Le mode "webview" n'est pas implémenté : on ne simule rien et on ne compte rien
            if !enabled || mode != "post_message" {
                thread::sleep(Duration::from_millis(200));
                continue;
            }

            let mut clicked = false;
            {
                unsafe {
                    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
                    use windows::Win32::UI::WindowsAndMessaging::{
                        FindWindowW, PostMessageW, GetClientRect, WM_LBUTTONDOWN, WM_LBUTTONUP,
                    };
                    use windows::core::HSTRING;

                    let window_name = HSTRING::from(target_title.as_str());
                    let hwnd = FindWindowW(None, &window_name).unwrap_or(HWND::default());
                    let target_hwnd = if !hwnd.0.is_null() {
                        hwnd
                    } else {
                        find_window_by_substring(&target_title).unwrap_or(HWND::default())
                    };

                    if !target_hwnd.0.is_null() {
                        let mut rect = windows::Win32::Foundation::RECT::default();
                        let _ = GetClientRect(target_hwnd, &mut rect);
                        let base_x = (rect.right - rect.left) / 2;
                        let base_y = (rect.bottom - rect.top) / 2;

                        let (x, y) = if jitter {
                            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros();
                            let offset_x = ((now % 11) as i32) - 5;
                            let offset_y = (((now / 13) % 11) as i32) - 5;
                            (base_x + offset_x, base_y + offset_y)
                        } else {
                            (base_x, base_y)
                        };

                        let lparam = LPARAM(((y & 0xFFFF) << 16 | (x & 0xFFFF)) as isize);
                        let wparam = WPARAM(1); // MK_LBUTTON

                        let _ = PostMessageW(target_hwnd, WM_LBUTTONDOWN, wparam, lparam);
                        thread::sleep(Duration::from_millis(20));
                        let _ = PostMessageW(target_hwnd, WM_LBUTTONUP, WPARAM(0), lparam);
                        click_count += 1;
                        clicked = true;
                    }
                }
            }

            // Fenêtre cible introuvable : on attend sans toucher à la jauge ni aux pauses
            if !clicked {
                thread::sleep(Duration::from_millis(500));
                continue;
            }

            if click_count % 5 == 0 {
                if let Ok(mut st) = state.lock() {
                    if st.like_goal.active {
                        st.like_goal.current_likes += 5;
                    }
                }
            }

            if pauses && click_count % 75 == 0 {
                thread::sleep(Duration::from_millis(2400));
            }

            let base_interval = 1000 / cps;
            let final_sleep = if jitter {
                let jitter_ms = (click_count % 7) as u64 * 6;
                base_interval as u64 + jitter_ms
            } else {
                base_interval as u64
            };
            thread::sleep(Duration::from_millis(final_sleep));
        }
    });
}

fn main() -> eframe::Result<()> {
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(app_path("run_diag.log")) {
            let _ = writeln!(f, "FATAL PANIC: {:?}", info);
        }
    }));

    let _ = std::fs::write(app_path("run_diag.log"), "ETAPE 1: main commence\n");

    unsafe {
        use windows::Win32::Foundation::{SetLastError, WIN32_ERROR};
        SetLastError(WIN32_ERROR(0));
    }

    let mutex_name = windows::core::HSTRING::from("Local\\BriserLeSilenceTelecommandeRustMutex_v1");
    let _mutex_guard = unsafe {
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;

        match CreateMutexW(None, true, &mutex_name) {
            Ok(handle) => {
                let err = GetLastError();
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
                    let _ = writeln!(f, "ETAPE 2: CreateMutexW Ok, err = {:?}", err);
                }
                if err == ERROR_ALREADY_EXISTS {
                    if bring_existing_window_to_front() {
                        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
                            let _ = writeln!(f, "ETAPE 2bis: FENETRE VISIBLE EXISTE -> FOCUS ET EXIT");
                        }
                        std::process::exit(0);
                    } else {
                        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
                            let _ = writeln!(f, "ETAPE 2bis: MUTEX EXISTE MAIS PAS DE FENETRE -> ON CONTINUE LE LANCEMENT");
                        }
                    }
                }
                handle
            }
            Err(e) => {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
                    let _ = writeln!(f, "ETAPE 2 ERR: {:?}", e);
                }
                windows::Win32::Foundation::HANDLE::default()
            }
        }
    };

    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
        let _ = writeln!(f, "ETAPE 3: Mutex passe avec succes");
    }

    let initial_state = if let Ok(raw) = std::fs::read_to_string(app_path("live_state.json")) {
        serde_json::from_str::<FullLiveState>(&raw).unwrap_or_default()
    } else {
        FullLiveState::default()
    };
    let state = Arc::new(Mutex::new(initial_state));
    let audio_peak_atomic = Arc::new(std::sync::atomic::AtomicU32::new(0));

    let state_http = Arc::clone(&state);
    thread::spawn(move || {
        run_http_server(state_http);
    });

    let state_media = Arc::clone(&state);
    thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let mut last_key = String::new();
        let mut cached_b64 = String::new();
        let mut cached_bytes = Vec::new();
        let mut last_title = String::new();
        let mut last_artist = String::new();
        let mut last_playing = false;
        let mut last_connected = false;
        loop {
            let info = read_windows_media_session(&mut last_key, &mut cached_b64, &mut cached_bytes);
            let has_changed = info.title != last_title
                || info.artist != last_artist
                || info.playing != last_playing
                || info.connected != last_connected;

            if has_changed {
                last_title = info.title.clone();
                last_artist = info.artist.clone();
                last_playing = info.playing;
                last_connected = info.connected;

                if let Ok(mut st) = state_media.lock() {
                    st.spotify = info;
                    save_state_json(&st);
                }
            }
            thread::sleep(Duration::from_millis(600));
        }
    });

    let audio_atomic_clone = Arc::clone(&audio_peak_atomic);
    // Raccourcis globaux : scrutés toutes les 30 ms hors de l'interface (marchent régie réduite),
    // une action par nouvel appui (front montant), jamais en rafale si la touche reste enfoncée
    let state_hotkeys = Arc::clone(&state);
    thread::spawn(move || {
        let mut previously_down: Vec<&'static str> = Vec::new();
        loop {
            thread::sleep(Duration::from_millis(30));
            let down = global_hotkeys_down();
            for action in down.iter().filter(|a| !previously_down.contains(a)) {
                if let Ok(mut st) = state_hotkeys.lock() {
                    apply_hotkey(&mut st, action);
                }
            }
            previously_down = down;
        }
    });

    // Journal du live : un seul observateur compare l'état toutes les 500 ms et note les changements
    // (branche, bouclier, chrono, épingle, sondage, demandes d'aide) sans toucher aux autres modules
    let state_journal = Arc::clone(&state);
    thread::spawn(move || {
        let snapshot = |st: &FullLiveState| {
            (
                st.branch.clone(),
                st.shield_active,
                st.timer_active && st.timer_running,
                st.timer_end_ts,
                if st.pinned_visible { st.pinned_text.clone() } else { String::new() },
                st.poll.active,
                st.help_alert_seq,
            )
        };
        let mut prev = {
            let mut st = state_journal.lock().unwrap();
            if st.journal.start_ts == 0.0 {
                journal_reset(&mut st);
            }
            snapshot(&st)
        };
        loop {
            thread::sleep(Duration::from_millis(500));
            let Ok(mut st) = state_journal.lock() else { continue };
            let cur = snapshot(&st);
            if cur == prev {
                continue;
            }
            if cur.0 != prev.0 {
                let line = format!("Branche → **{}** ({})", cur.0, st.custom_topic);
                journal_log(&mut st, line);
            }
            if cur.1 != prev.1 {
                journal_log(&mut st, if cur.1 { "🛡️ Bouclier anti-raid **activé**".into() } else { "🛡️ Bouclier levé".into() });
            }
            // Nouveau chrono lancé (pas une reprise après pause) : l'heure de fin change et le restant = durée totale
            let fresh_start = (st.timer_end_ts - now_secs() - st.timer_total_secs as f64).abs() < 2.0;
            if cur.2 && cur.3 != prev.3 && fresh_start {
                let who = if st.timer_label.is_empty() { "temps de parole".to_string() } else { st.timer_label.clone() };
                let line = format!("⏱️ Chrono {} min — {}", (st.timer_total_secs + 30) / 60, who);
                journal_log(&mut st, line);
            }
            if !cur.4.is_empty() && cur.4 != prev.4 {
                let author = st.pinned_author.clone();
                journal_log(&mut st, format!("📌 Épinglé ({}) : « {} »", author, cur.4));
            }
            if cur.5 != prev.5 {
                let p = st.poll.clone();
                if cur.5 {
                    journal_log(&mut st, format!("🗳️ Sondage lancé : « {} »", p.question));
                } else {
                    journal_log(
                        &mut st,
                        format!(
                            "🗳️ Sondage terminé : « {} » — {} {} / {} {} / {} {} / {} {}",
                            p.question, p.choice1, p.votes1, p.choice2, p.votes2, p.choice3, p.votes3, p.choice4, p.votes4
                        ),
                    );
                }
            }
            if cur.6 > prev.6 {
                let n = cur.6 - prev.6;
                journal_log(&mut st, format!("🆘 {} nouvelle(s) demande(s) d'aide (détails en fin de journal)", n));
            }
            save_state_json(&st);
            prev = cur;
        }
    });

    // Chrono de débat : gong à 0, puis le widget disparaît 8 s après la fin
    let state_timer = Arc::clone(&state);
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(200));
        if let Ok(mut st) = state_timer.lock() {
            if !st.timer_active || !st.timer_running {
                continue;
            }
            let now = now_secs();
            if now >= st.timer_end_ts && !st.timer_gong_done {
                st.timer_gong_done = true;
                st.jingle_cmd_id += 1;
                st.jingle_type = "synth".to_string();
                st.jingle_key = "gong".to_string();
                st.jingle_file_path = String::new();
                st.jingle_volume = 80;
                save_state_json(&st);
            }
            if now >= st.timer_end_ts + 8.0 {
                st.timer_active = false;
                st.timer_running = false;
                save_state_json(&st);
            }
        }
    });

    let state_audio = Arc::clone(&state);
    thread::spawn(move || {
        let meter = SystemAudioMeter::new();
        let mut last_sync = Instant::now();
        let mut rolling_peak: f32 = 0.0;
        loop {
            let peak = meter.get_peak();
            if peak > rolling_peak {
                rolling_peak = peak;
            } else {
                rolling_peak = (rolling_peak * 0.94).max(0.0);
            }

            audio_atomic_clone.store(rolling_peak.to_bits(), std::sync::atomic::Ordering::Relaxed);

            // Synchronisation haute réactivité (50ms) pour les overlays web OBS / TikTok Studio
            if last_sync.elapsed() >= Duration::from_millis(50) {
                last_sync = Instant::now();
                if let Ok(mut st) = state_audio.lock() {
                    st.audio_peak = rolling_peak;
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
    });

    run_background_tapper(Arc::clone(&state));
    let state_for_startup = Arc::clone(&state);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(600));
        let fmt = if let Ok(st) = state_for_startup.lock() {
            st.preferred_stage_format.clone()
        } else {
            "9_16".to_string()
        };
        if fmt == "16_9" {
            TelecommandeApp::open_stage_16_9_window();
        } else {
            TelecommandeApp::open_stage_9_16_window();
        }
    });

    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
        let _ = writeln!(f, "ETAPE 4: Avant run_native...");
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("🎛️ TELECOMMANDE_RUST — Briser le Silence")
            .with_inner_size([540.0, 720.0])
            .with_min_inner_size([460.0, 500.0])
            .with_resizable(true)
            .with_always_on_top(),
        ..Default::default()
    };

    let audio_peak_app = Arc::clone(&audio_peak_atomic);
    let res = eframe::run_native(
        "TELECOMMANDE_RUST",
        options,
        Box::new(move |cc| {
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
                let _ = writeln!(f, "ETAPE 5: Creation de TelecommandeApp dans run_native...");
            }
            install_windows_emoji_fonts(&cc.egui_ctx);
            Ok(Box::new(TelecommandeApp::new(state, audio_peak_app)))
        }),
    );

    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(app_path("run_diag.log")) {
        let _ = writeln!(f, "ETAPE 6: Fin run_native: {:?}", res);
    }
    res
}
