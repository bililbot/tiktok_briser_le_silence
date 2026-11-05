# -*- coding: utf-8 -*-
"""
TÉLÉCOMMANDE CLIENT + FENÊTRE DE CAPTURE MUSIQUE DÉDIÉE POUR TIKTOK LIVE STUDIO
Projet TikTok LIVE : « Et si... BRISER LE SILENCE »
- Fenêtre 1 : TELECOMMANDE_CONTROLE (pour piloter les branches F9/F10/F11 et la musique)
- Fenêtre 2 : MUSIQUE_LIVE_TIKTOK (fenêtre dédiée à part avec tour explicite à capturer dans TikTok LIVE Studio)
  -> Affiche UNIQUEMENT : Pochette + Titre + Artiste + Album + Spectre animé (SANS mention de Spotify)
"""

import ctypes
import io
import json
import math
import os
import subprocess
import threading
import time
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer
import tkinter as tk
import webbrowser
from PIL import Image, ImageDraw, ImageTk

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
STATE_FILE = os.path.join(BASE_DIR, "live_state.json")

# État partagé
LIVE_STATE = {
    "branch": "musique",
    "branch_timestamp": time.time(),
    "custom_topic": "",
    "custom_sub": "",
    "spotify": {
        "connected": False,
        "playing": False,
        "title": "En attente d'un morceau...",
        "artist": "Underground Insurrection",
        "album": "",
        "cover_url": "",
        "genre": "ENGAGÉ"
    }
}

COVER_CACHE = {}
IMAGE_BYTES_CACHE = {}


def fetch_cover_and_genre(artist: str, title: str, album: str):
    """Récupère la pochette HD et le style via l'API publique iTunes."""
    key = f"{artist}||{title}||{album}".strip().lower()
    if not artist and not title:
        return "", ""
    if key in COVER_CACHE:
        return COVER_CACHE[key]

    try:
        query = urllib.parse.quote(f"{artist} {title}".strip())
        url = f"https://itunes.apple.com/search?term={query}&entity=song&limit=1"
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=3) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            results = data.get("results", [])
            if results:
                r = results[0]
                cover = r.get("artworkUrl100", "").replace("100x100bb", "300x300bb")
                genre = r.get("primaryGenreName", "ENGAGÉ").upper()
                COVER_CACHE[key] = (cover, genre)
                return cover, genre
    except Exception:
        pass

    COVER_CACHE[key] = ("", "UNDERGROUND")
    return "", "UNDERGROUND"


def fetch_cover_pil(cover_url: str, size=(96, 96)) -> Image.Image:
    """Télécharge la pochette ou génère un vinyle Underground par défaut."""
    if cover_url and cover_url in IMAGE_BYTES_CACHE:
        try:
            img = Image.open(io.BytesIO(IMAGE_BYTES_CACHE[cover_url])).convert("RGB")
            return img.resize(size, Image.Resampling.LANCZOS)
        except Exception:
            pass

    if cover_url:
        try:
            req = urllib.request.Request(cover_url, headers={"User-Agent": "Mozilla/5.0"})
            with urllib.request.urlopen(req, timeout=3) as resp:
                raw = resp.read()
                IMAGE_BYTES_CACHE[cover_url] = raw
                img = Image.open(io.BytesIO(raw)).convert("RGB")
                return img.resize(size, Image.Resampling.LANCZOS)
        except Exception:
            pass

    # Image vinyle par défaut si pas de pochette
    img = Image.new("RGB", size, "#181920")
    draw = ImageDraw.Draw(img)
    cx, cy = size[0] // 2, size[1] // 2
    draw.ellipse([8, 8, size[0] - 8, size[1] - 8], fill="#0d0e12", outline="#f97316", width=2)
    draw.ellipse([cx - 12, cy - 12, cx + 12, cy + 12], fill="#f97316", outline="#ffffff", width=1)
    return img


def save_state_to_disk():
    try:
        tmp = STATE_FILE + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(LIVE_STATE, f, ensure_ascii=False, indent=2)
        os.replace(tmp, STATE_FILE)
    except Exception:
        pass


def poll_spotify_loop(ui_callback=None):
    """Interroge la session média Windows toutes les 1.8s."""
    startupinfo = subprocess.STARTUPINFO()
    startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    ps_script_file = os.path.join(BASE_DIR, "get_spotify.ps1")

    while True:
        try:
            proc = subprocess.run(
                ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ps_script_file],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=4,
                startupinfo=startupinfo
            )
            raw = proc.stdout.strip()
            if raw.startswith("{"):
                data = json.loads(raw)
                if data.get("connected") and data.get("title"):
                    title = data.get("title", "").strip()
                    artist = data.get("artist", "").strip()
                    album = data.get("album", "").strip()
                    playing = bool(data.get("playing", True))

                    cover_url, genre = fetch_cover_and_genre(artist, title, album)

                    LIVE_STATE["spotify"] = {
                        "connected": True,
                        "playing": playing,
                        "title": title,
                        "artist": artist,
                        "album": album,
                        "cover_url": cover_url,
                        "genre": genre
                    }
                else:
                    LIVE_STATE["spotify"]["connected"] = False
        except Exception:
            pass

        save_state_to_disk()
        if ui_callback:
            try:
                ui_callback()
            except Exception:
                pass
        time.sleep(1.8)


# =====================================================================
# SERVEUR HTTP LOCAL (PORT 8765)
# =====================================================================
class StateHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        return

    def _send_cors(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")

    def do_OPTIONS(self):
        self.send_response(200)
        self._send_cors()
        self.end_headers()

    def do_GET(self):
        if self.path.startswith("/state"):
            self.send_response(200)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self._send_cors()
            self.end_headers()
            self.wfile.write(json.dumps(LIVE_STATE, ensure_ascii=False).encode("utf-8"))
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        if self.path.startswith("/update"):
            length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(length).decode("utf-8") if length > 0 else "{}"
            try:
                payload = json.loads(body)
                if "branch" in payload:
                    LIVE_STATE["branch"] = payload["branch"]
                    LIVE_STATE["branch_timestamp"] = time.time()
                if "custom_topic" in payload:
                    LIVE_STATE["custom_topic"] = payload["custom_topic"]
                save_state_to_disk()
            except Exception:
                pass
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self._send_cors()
            self.end_headers()
            self.wfile.write(b'{"ok":true}')


def start_http_server():
    try:
        server = HTTPServer(("127.0.0.1", 8765), StateHandler)
        server.serve_forever()
    except Exception:
        pass


# =====================================================================
# TOUCHES MÉDIA & RACCOURCIS CLAVIER GLOBAUX (F9, F10, F11)
# =====================================================================
VK_MEDIA_NEXT_TRACK = 0xB0
VK_MEDIA_PREV_TRACK = 0xB1
VK_MEDIA_PLAY_PAUSE = 0xB3


def send_media_key(vk_code):
    ctypes.windll.user32.keybd_event(vk_code, 0, 0, 0)
    ctypes.windll.user32.keybd_event(vk_code, 0, 2, 0)


def global_hotkeys_loop(switch_callback):
    VK_F9 = 0x78
    VK_F10 = 0x79
    VK_F11 = 0x7A
    last_pressed = 0
    while True:
        now = time.time()
        if now - last_pressed > 0.4:
            if ctypes.windll.user32.GetAsyncKeyState(VK_F9) & 0x8000:
                switch_callback("vss")
                last_pressed = now
            elif ctypes.windll.user32.GetAsyncKeyState(VK_F10) & 0x8000:
                switch_callback("politique")
                last_pressed = now
            elif ctypes.windll.user32.GetAsyncKeyState(VK_F11) & 0x8000:
                switch_callback("musique")
                last_pressed = now
        time.sleep(0.08)


# =====================================================================
# FENÊTRE À PART N°2 : "MUSIQUE_LIVE_TIKTOK"
# (Fenêtre dédiée à capturer dans TikTok LIVE Studio > Ajouter une source > Fenêtre)
# =====================================================================
class MusicOverlayWindow:
    def __init__(self, master):
        self.win = tk.Toplevel(master)
        # TITRE EXPLICITE POUR TIKTOK LIVE STUDIO
        self.win.title("MUSIQUE_LIVE_TIKTOK")
        self.win.geometry("540x136+60+60")
        self.win.configure(bg="#f97316")  # Couleur du tour de fenêtre explicite
        self.win.resizable(True, True)
        self.borderless = False
        self.accent_color = "#f97316"
        self.secondary_color = "#fbbf24"
        self.last_cover_url = None
        self.tk_cover = None

        # Permettre de déplacer la fenêtre même si on masque la barre Windows
        self._drag_x = 0
        self._drag_y = 0

        # CADRE EXTÉRIEUR EXPLICITE (TOUR DE LA FENÊTRE VISIBLE DANS TIKTOK STUDIO)
        self.outer_border = tk.Frame(
            self.win,
            bg="#f97316",
            padx=4,
            pady=4
        )
        self.outer_border.pack(fill="both", expand=True)

        # Liseré intérieur sombre + fond béton #0e0f14
        self.inner_box = tk.Frame(
            self.outer_border,
            bg="#0e0f14",
            highlightbackground="#fbbf24",
            highlightcolor="#fbbf24",
            highlightthickness=1,
            padx=10,
            pady=8
        )
        self.inner_box.pack(fill="both", expand=True)

        # Permettre le glisser-déposer de la fenêtre à la souris
        for w in (self.win, self.outer_border, self.inner_box):
            w.bind("<ButtonPress-1>", self._start_move)
            w.bind("<B1-Motion>", self._on_move)

        # --- GAUCHE : POCHETTE DE L'ALBUM (96x96) ---
        self.cover_border = tk.Frame(self.inner_box, bg="#f97316", padx=2, pady=2)
        self.cover_border.pack(side="left", padx=(0, 12))

        self.cover_label = tk.Label(self.cover_border, bg="#181920", width=96, height=96)
        self.cover_label.pack()
        self.update_cover_image("")

        # --- DROITE : INFOS MORCEAU (SANS MENTION DE SPOTIFY) + SPECTRE ---
        self.info_frame = tk.Frame(self.inner_box, bg="#0e0f14")
        self.info_frame.pack(side="left", fill="both", expand=True)

        # 1. TITRE DU MORCEAU (Gros & Impactant)
        self.lbl_song_title = tk.Label(
            self.info_frame,
            text="SOVIET SUPREM PARTY",
            bg="#0e0f14",
            fg="#ffffff",
            font=("Impact", 17),
            anchor="w"
        )
        self.lbl_song_title.pack(fill="x", pady=(0, 1))

        # 2. ARTISTE
        self.lbl_song_artist = tk.Label(
            self.info_frame,
            text="🎤 SOVIET SUPREM",
            bg="#0e0f14",
            fg="#fbbf24",
            font=("Segoe UI", 11, "bold"),
            anchor="w"
        )
        self.lbl_song_artist.pack(fill="x")

        # 3. ALBUM
        self.lbl_song_album = tk.Label(
            self.info_frame,
            text="💿 Album : L'Internationale",
            bg="#0e0f14",
            fg="#d1d5db",
            font=("Segoe UI", 9, "bold"),
            anchor="w"
        )
        self.lbl_song_album.pack(fill="x", pady=(1, 4))

        # 4. SPECTRE AUDIO ANIMÉ EN BAS
        self.spec_canvas = tk.Canvas(
            self.info_frame,
            bg="#090a0d",
            height=22,
            highlightthickness=1,
            highlightbackground="#262833"
        )
        self.spec_canvas.pack(fill="x")

        self.animate_spectrum()

    def _start_move(self, event):
        self._drag_x = event.x
        self._drag_y = event.y

    def _on_move(self, event):
        x = self.win.winfo_x() + (event.x - self._drag_x)
        y = self.win.winfo_y() + (event.y - self._drag_y)
        self.win.geometry(f"+{x}+{y}")

    def toggle_borderless(self):
        """Active/désactive la barre de titre Windows tout en gardant le tour coloré explicite."""
        self.borderless = not self.borderless
        self.win.overrideredirect(self.borderless)

    def set_frame_colors(self, primary: str, secondary: str):
        self.accent_color = primary
        self.secondary_color = secondary
        self.win.configure(bg=primary)
        self.outer_border.configure(bg=primary)
        self.inner_box.configure(highlightbackground=secondary, highlightcolor=secondary)
        self.cover_border.configure(bg=primary)
        self.lbl_song_artist.configure(fg=secondary)

    def update_cover_image(self, cover_url: str):
        if cover_url == self.last_cover_url and self.tk_cover is not None:
            return
        self.last_cover_url = cover_url
        pil_img = fetch_cover_pil(cover_url, size=(96, 96))
        self.tk_cover = ImageTk.PhotoImage(pil_img)
        self.cover_label.configure(image=self.tk_cover)

    def update_track_display(self, sp: dict):
        if sp.get("connected"):
            title = (sp.get("title") or "MORCEAU EN COURS").upper()
            artist = (sp.get("artist") or "ARTISTE").upper()
            album = sp.get("album") or "Single"
            self.lbl_song_title.configure(text=title)
            self.lbl_song_artist.configure(text=f"🎤 {artist}")
            self.lbl_song_album.configure(text=f"💿 Album : {album}")
            self.update_cover_image(sp.get("cover_url", ""))
        else:
            self.lbl_song_title.configure(text="UNDERGROUND INSURRECTION")
            self.lbl_song_artist.configure(text="🎤 EN ATTENTE DE MUSIQUE")
            self.lbl_song_album.configure(text="💿 Lance un morceau sur ton lecteur")

    def animate_spectrum(self):
        try:
            self.spec_canvas.delete("all")
            w = max(280, self.spec_canvas.winfo_width())
            h = 22
            bar_count = 28
            bar_w = (w / bar_count) - 3
            playing = LIVE_STATE.get("spotify", {}).get("playing", False)
            t = time.time() * 10.0

            for i in range(bar_count):
                if playing:
                    v1 = abs(math.sin(t + i * 0.45)) * 0.65
                    v2 = abs(math.cos(t * 1.3 - i * 0.3)) * 0.35
                    bh = max(3, int((v1 + v2) * (h - 3)))
                else:
                    bh = 3 + int(abs(math.sin(t * 0.2 + i * 0.5)) * 3)

                x1 = i * (bar_w + 3) + 2
                y1 = h - bh
                x2 = x1 + bar_w
                y2 = h
                color = self.secondary_color if bh > (h * 0.65) else self.accent_color
                self.spec_canvas.create_rectangle(x1, y1, x2, y2, fill=color, outline="")
        except Exception:
            pass
        self.win.after(70, self.animate_spectrum)


# =====================================================================
# FENÊTRE N°1 : TÉLÉCOMMANDE DE CONTRÔLE (POUR TOI)
# =====================================================================
class TelecommandeLiveApp:
    def __init__(self, root):
        self.root = root
        self.root.title("🎛️ TELECOMMANDE_CONTROLE — Briser le Silence")
        self.root.geometry("470x560+620+60")
        self.root.configure(bg="#111216")
        self.root.attributes("-topmost", True)

        # En-tête
        header = tk.Frame(root, bg="#191b22", padx=14, pady=10)
        header.pack(fill="x")
        tk.Label(
            header,
            text="🎛️ TÉLÉCOMMANDE LIVE — BRISER LE SILENCE",
            bg="#191b22",
            fg="#ffffff",
            font=("Segoe UI", 11, "bold")
        ).pack(anchor="w")
        tk.Label(
            header,
            text="Dans TikTok Studio > Ajouter une source > Fenêtre > Choisis : MUSIQUE_LIVE_TIKTOK",
            bg="#191b22",
            fg="#fbbf24",
            font=("Segoe UI", 8, "bold")
        ).pack(anchor="w")

        # 1. LES 3 GROS BOUTONS DE BRANCHE
        branch_frame = tk.LabelFrame(
            root,
            text=" ⚡ 1. CHOISIR LA BRANCHE EN DIRECT (CLIC OU F9 / F10 / F11) ",
            bg="#111216",
            fg="#eab308",
            font=("Segoe UI", 9, "bold"),
            padx=10,
            pady=8
        )
        branch_frame.pack(fill="x", padx=12, pady=6)

        self.btn_vss = tk.Button(
            branch_frame,
            text="💜 [F9] BRANCHE 1 : INCESTE & VSS (Survivant·e·s)",
            bg="#2e1a47",
            fg="#ffffff",
            font=("Segoe UI", 10, "bold"),
            relief="flat",
            pady=7,
            cursor="hand2",
            command=lambda: self.set_branch("vss")
        )
        self.btn_vss.pack(fill="x", pady=3)

        self.btn_pol = tk.Button(
            branch_frame,
            text="✊ [F10] BRANCHE 2 : POLITIQUE (Antifa • Féministe • Anticapitaliste)",
            bg="#451a1c",
            fg="#ffffff",
            font=("Segoe UI", 10, "bold"),
            relief="flat",
            pady=7,
            cursor="hand2",
            command=lambda: self.set_branch("politique")
        )
        self.btn_pol.pack(fill="x", pady=3)

        self.btn_mus = tk.Button(
            branch_frame,
            text="🎸 [F11] BRANCHE 3 : MUSIQUE (Underground Insurrection)",
            bg="#ea580c",
            fg="#ffffff",
            font=("Segoe UI", 10, "bold"),
            relief="flat",
            pady=7,
            cursor="hand2",
            command=lambda: self.set_branch("musique")
        )
        self.btn_mus.pack(fill="x", pady=3)

        # 2. CONTRÔLE DE LA FENÊTRE MUSIQUE À PART ("MUSIQUE_LIVE_TIKTOK")
        spot_frame = tk.LabelFrame(
            root,
            text=" 🎵 2. FENÊTRE MUSIQUE À PART (« MUSIQUE_LIVE_TIKTOK ») ",
            bg="#111216",
            fg="#22c55e",
            font=("Segoe UI", 9, "bold"),
            padx=12,
            pady=8
        )
        spot_frame.pack(fill="x", padx=12, pady=6)

        self.lbl_title = tk.Label(
            spot_frame,
            text="Titre : Recherche...",
            bg="#111216",
            fg="#ffffff",
            font=("Segoe UI", 10, "bold"),
            anchor="w"
        )
        self.lbl_title.pack(fill="x")

        self.lbl_artist = tk.Label(
            spot_frame,
            text="Artiste : - | Album : -",
            bg="#111216",
            fg="#fb923c",
            font=("Segoe UI", 9),
            anchor="w"
        )
        self.lbl_artist.pack(fill="x", pady=(0, 6))

        # Boutons Play / Pause / Suivant
        ctrl_row = tk.Frame(spot_frame, bg="#111216")
        ctrl_row.pack(fill="x", pady=(0, 6))
        tk.Button(
            ctrl_row, text="⏮️ Précédent", bg="#252730", fg="#fff", relief="flat", padx=10, pady=4,
            command=lambda: send_media_key(VK_MEDIA_PREV_TRACK)
        ).pack(side="left", expand=True, fill="x", padx=2)
        tk.Button(
            ctrl_row, text="⏯️ Play / Pause", bg="#16a34a", fg="#fff", font=("Segoe UI", 9, "bold"), relief="flat", padx=10, pady=4,
            command=lambda: send_media_key(VK_MEDIA_PLAY_PAUSE)
        ).pack(side="left", expand=True, fill="x", padx=2)
        tk.Button(
            ctrl_row, text="⏭️ Suivant", bg="#252730", fg="#fff", relief="flat", padx=10, pady=4,
            command=lambda: send_media_key(VK_MEDIA_NEXT_TRACK)
        ).pack(side="left", expand=True, fill="x", padx=2)

        # Options de la fenêtre musique indépendante
        win_opt_row = tk.Frame(spot_frame, bg="#111216")
        win_opt_row.pack(fill="x")
        tk.Button(
            win_opt_row,
            text="🎵 Ouvrir / Relancer la fenêtre « MUSIQUE_LIVE_TIKTOK »",
            bg="#2d313e",
            fg="#fbbf24",
            font=("Segoe UI", 8, "bold"),
            relief="flat",
            pady=5,
            command=self.launch_music_window
        ).pack(fill="x")

        # 3. CHANGER LE SUJET DU LIVE À LA VOLÉE
        topic_frame = tk.LabelFrame(
            root,
            text=" ✍️ 3. SUJET / MESSAGE AFFICHÉ SUR L'ÉCRAN LIVE ",
            bg="#111216",
            fg="#f3f4f6",
            font=("Segoe UI", 9, "bold"),
            padx=12,
            pady=8
        )
        topic_frame.pack(fill="x", padx=12, pady=6)

        self.entry_topic = tk.Entry(
            topic_frame,
            bg="#1e2028",
            fg="#ffffff",
            insertbackground="#ffffff",
            font=("Segoe UI", 9),
            relief="flat"
        )
        self.entry_topic.pack(fill="x", ipady=5, pady=(0, 6))
        self.entry_topic.insert(0, "SONS ENGAGÉS : PUNK • TECHNO • HARDCORE • HIP-HOP")

        btn_row = tk.Frame(topic_frame, bg="#111216")
        btn_row.pack(fill="x")
        tk.Button(
            btn_row,
            text="✅ Envoyer ce sujet sur le Live",
            bg="#ea580c",
            fg="#ffffff",
            font=("Segoe UI", 9, "bold"),
            relief="flat",
            pady=5,
            command=self.update_custom_topic
        ).pack(side="left", expand=True, fill="x", padx=(0, 4))

        tk.Button(
            btn_row,
            text="🌐 Ouvrir Studio 9:16",
            bg="#252730",
            fg="#ffffff",
            font=("Segoe UI", 9),
            relief="flat",
            pady=5,
            command=lambda: webbrowser.open(os.path.join(BASE_DIR, "index.html"))
        ).pack(side="left", padx=(4, 0))

        # Barre du bas : Toujours au premier plan
        footer = tk.Frame(root, bg="#111216", padx=12, pady=4)
        footer.pack(fill="x")
        self.topmost_var = tk.BooleanVar(value=True)
        tk.Checkbutton(
            footer,
            text="Garder la télécommande toujours au-dessus",
            variable=self.topmost_var,
            bg="#111216",
            fg="#9ca3af",
            selectcolor="#111216",
            activebackground="#111216",
            activeforeground="#ffffff",
            command=self.toggle_topmost
        ).pack(anchor="w")

        self.highlight_active_branch()

    def toggle_topmost(self):
        self.root.attributes("-topmost", self.topmost_var.get())

    def set_branch(self, branch_key):
        LIVE_STATE["branch"] = branch_key
        LIVE_STATE["branch_timestamp"] = time.time()
        default_topics = {
            "vss": "BRISER L'OMERTA SUR L'INCESTE ET LES VIOLENCES SEXUELLES",
            "politique": "CAPITALISME, PATRIARCAT & FASCISME : MÊME COMBAT, MÊME RIPOSTE",
            "musique": "SONS ENGAGÉS : PUNK • TECHNO • HARDCORE • HIP-HOP • METAL"
        }
        branch_colors = {
            "vss": ("#a855f7", "#fbbf24"),
            "politique": ("#ef4444", "#fbbf24"),
            "musique": ("#f97316", "#fbbf24")
        }
        LIVE_STATE["custom_topic"] = default_topics.get(branch_key, "")
        self.entry_topic.delete(0, tk.END)
        self.entry_topic.insert(0, LIVE_STATE["custom_topic"])
        save_state_to_disk()
        self.root.after(0, self.highlight_active_branch)

    def launch_music_window(self):
        music_script = os.path.join(BASE_DIR, "fenetre_musique_tiktok.py")
        subprocess.Popen(["pythonw", music_script], cwd=BASE_DIR)

    def update_custom_topic(self):
        LIVE_STATE["custom_topic"] = self.entry_topic.get().strip()
        LIVE_STATE["branch_timestamp"] = time.time()
        save_state_to_disk()

    def highlight_active_branch(self):
        b = LIVE_STATE["branch"]
        self.btn_vss.configure(bg="#9333ea" if b == "vss" else "#231934")
        self.btn_pol.configure(bg="#dc2626" if b == "politique" else "#33181b")
        self.btn_mus.configure(bg="#ea580c" if b == "musique" else "#332015")

    def refresh_spotify_ui(self):
        sp = LIVE_STATE.get("spotify", {})
        if sp.get("connected"):
            status_icon = "▶️" if sp.get("playing") else "⏸️"
            self.lbl_title.configure(text=f"{status_icon} {sp.get('title', '-')}")
            alb = sp.get("album") or "Single"
            self.lbl_artist.configure(text=f"🎤 {sp.get('artist', '-')}  •  💿 {alb}")
        else:
            self.lbl_title.configure(text="⏸️ Aucun morceau en lecture")


if __name__ == "__main__":
    save_state_to_disk()

    t_http = threading.Thread(target=start_http_server, daemon=True)
    t_http.start()

    root = tk.Tk()
    app = TelecommandeLiveApp(root)

    t_spot = threading.Thread(
        target=poll_spotify_loop,
        args=(lambda: root.after(0, app.refresh_spotify_ui),),
        daemon=True
    )
    t_spot.start()

    t_keys = threading.Thread(
        target=global_hotkeys_loop,
        args=(app.set_branch,),
        daemon=True
    )
    t_keys.start()

    root.mainloop()
