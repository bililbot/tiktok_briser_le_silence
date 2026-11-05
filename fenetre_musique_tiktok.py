# -*- coding: utf-8 -*-
"""
FENÊTRE DÉDIÉE INDÉPENDANTE : MUSIQUE_LIVE_TIKTOK
À capturer directement dans TikTok LIVE Studio > Ajouter une source > Capture de fenêtre
- Processus indépendant : MUSIQUE_LIVE_TIKTOK
- Lecture directe & autonome de la musique en cours (WinRT Media Session + live_state.json)
- Zéro mention de Spotify
- Tour de fenêtre explicite (cadre néon/street-art double liseré)
- Affiche : Pochette d'album HD + Titre + Artiste + Album + Spectre audio animé
"""

import io
import json
import math
import os
import subprocess
import threading
import time
import urllib.parse
import urllib.request
import tkinter as tk
from PIL import Image, ImageDraw, ImageTk

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
STATE_FILE = os.path.join(BASE_DIR, "live_state.json")
PS_SCRIPT_FILE = os.path.join(BASE_DIR, "get_spotify.ps1")

COVER_URL_CACHE = {}
IMAGE_BYTES_CACHE = {}

BRANCH_COLORS = {
    "vss": ("#a855f7", "#fbbf24"),
    "politique": ("#ef4444", "#fbbf24"),
    "musique": ("#f97316", "#fbbf24")
}


def fetch_cover_url(artist: str, title: str) -> str:
    key = f"{artist}||{title}".strip().lower()
    if not artist and not title:
        return ""
    if key in COVER_URL_CACHE:
        return COVER_URL_CACHE[key]
    try:
        query = urllib.parse.quote(f"{artist} {title}".strip())
        url = f"https://itunes.apple.com/search?term={query}&entity=song&limit=1"
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=3) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            results = data.get("results", [])
            if results:
                cover = results[0].get("artworkUrl100", "").replace("100x100bb", "300x300bb")
                COVER_URL_CACHE[key] = cover
                return cover
    except Exception:
        pass
    COVER_URL_CACHE[key] = ""
    return ""


def fetch_cover_pil(cover_url: str, size=(100, 100)) -> Image.Image:
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

    img = Image.new("RGB", size, "#181920")
    draw = ImageDraw.Draw(img)
    cx, cy = size[0] // 2, size[1] // 2
    draw.ellipse([8, 8, size[0] - 8, size[1] - 8], fill="#0d0e12", outline="#f97316", width=2)
    draw.ellipse([cx - 12, cy - 12, cx + 12, cy + 12], fill="#f97316", outline="#ffffff", width=1)
    return img


class StandaloneMusicWindow:
    def __init__(self, root: tk.Tk):
        self.root = root
        self.root.title("MUSIQUE_LIVE_TIKTOK")
        self.root.geometry("550x140+50+50")
        self.root.configure(bg="#f97316")

        self.accent_color = "#f97316"
        self.secondary_color = "#fbbf24"
        self.tk_cover = None
        self.playing = False
        self.current_branch = "musique"

        self.current_title = ""
        self.current_artist = ""
        self.current_album = ""
        self.current_pil_img = fetch_cover_pil("", size=(100, 100))

        # CADRE EXTÉRIEUR EXPLICITE (TOUR DE LA FENÊTRE TRÈS VISIBLE)
        self.outer_border = tk.Frame(self.root, bg="#f97316", padx=5, pady=5)
        self.outer_border.pack(fill="both", expand=True)

        # CADRE INTÉRIEUR SOMBRE BÉTON + LISERÉ OR
        self.inner_box = tk.Frame(
            self.outer_border,
            bg="#0e0f14",
            highlightbackground="#fbbf24",
            highlightcolor="#fbbf24",
            highlightthickness=2,
            padx=10,
            pady=8
        )
        self.inner_box.pack(fill="both", expand=True)

        # GAUCHE : POCHETTE DE L'ALBUM (100x100)
        self.cover_border = tk.Frame(self.inner_box, bg="#f97316", padx=2, pady=2)
        self.cover_border.pack(side="left", padx=(0, 12))

        self.tk_cover = ImageTk.PhotoImage(self.current_pil_img)
        self.cover_label = tk.Label(self.cover_border, image=self.tk_cover, bg="#181920", width=100, height=100)
        self.cover_label.pack()

        # DROITE : TITRE + ARTISTE + ALBUM + SPECTRE (ZÉRO TEXTE SPOTIFY)
        self.info_frame = tk.Frame(self.inner_box, bg="#0e0f14")
        self.info_frame.pack(side="left", fill="both", expand=True)

        # 1. TITRE DU MORCEAU
        self.lbl_song_title = tk.Label(
            self.info_frame,
            text="CHARGEMENT DU MORCEAU...",
            bg="#0e0f14",
            fg="#ffffff",
            font=("Impact", 18),
            anchor="w"
        )
        self.lbl_song_title.pack(fill="x")

        # 2. ARTISTE
        self.lbl_song_artist = tk.Label(
            self.info_frame,
            text="🎤 ...",
            bg="#0e0f14",
            fg="#fbbf24",
            font=("Segoe UI", 11, "bold"),
            anchor="w"
        )
        self.lbl_song_artist.pack(fill="x")

        # 3. ALBUM
        self.lbl_song_album = tk.Label(
            self.info_frame,
            text="💿 Album : ...",
            bg="#0e0f14",
            fg="#d1d5db",
            font=("Segoe UI", 9, "bold"),
            anchor="w"
        )
        self.lbl_song_album.pack(fill="x", pady=(1, 5))

        # 4. SPECTRE AUDIO ANIMÉ
        self.spec_canvas = tk.Canvas(
            self.info_frame,
            bg="#090a0d",
            height=22,
            highlightthickness=1,
            highlightbackground="#262833"
        )
        self.spec_canvas.pack(fill="x")

        # Charger immédiatement depuis live_state.json s'il existe déjà
        self.load_initial_state()

        # Démarrer un thread autonome qui lit directement le morceau en cours
        t = threading.Thread(target=self.background_reader_loop, daemon=True)
        t.start()

        self.animate_spectrum()

    def load_initial_state(self):
        try:
            if os.path.exists(STATE_FILE):
                with open(STATE_FILE, "r", encoding="utf-8") as f:
                    st = json.load(f)
                sp = st.get("spotify", {})
                if sp.get("title"):
                    self.apply_song_to_ui(
                        sp.get("title", ""),
                        sp.get("artist", ""),
                        sp.get("album", ""),
                        bool(sp.get("playing", True)),
                        None
                    )
        except Exception:
            pass

    def set_frame_colors(self, primary: str, secondary: str):
        self.accent_color = primary
        self.secondary_color = secondary
        self.root.configure(bg=primary)
        self.outer_border.configure(bg=primary)
        self.inner_box.configure(highlightbackground=secondary, highlightcolor=secondary)
        self.cover_border.configure(bg=primary)
        self.lbl_song_artist.configure(fg=secondary)

    def apply_song_to_ui(self, title: str, artist: str, album: str, playing: bool, pil_img):
        self.playing = playing
        self.lbl_song_title.configure(text=(title or "MORCEAU EN COURS").upper())
        self.lbl_song_artist.configure(text=f"🎤 {(artist or 'ARTISTE').upper()}")
        self.lbl_song_album.configure(text=f"💿 Album : {album or 'Single'}")
        if pil_img is not None:
            self.tk_cover = ImageTk.PhotoImage(pil_img)
            self.cover_label.configure(image=self.tk_cover)

    def background_reader_loop(self):
        """Lit directement la session média Windows de façon 100% autonome."""
        startupinfo = subprocess.STARTUPINFO()
        startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
        last_cover_key = None

        while True:
            try:
                # 1. Vérifier la couleur de branche dans live_state.json
                if os.path.exists(STATE_FILE):
                    try:
                        with open(STATE_FILE, "r", encoding="utf-8") as f:
                            st = json.load(f)
                        br = st.get("branch", "musique")
                        if br != self.current_branch:
                            self.current_branch = br
                            p_col, s_col = BRANCH_COLORS.get(br, ("#f97316", "#fbbf24"))
                            self.root.after(0, lambda p=p_col, s=s_col: self.set_frame_colors(p, s))
                    except Exception:
                        pass

                # 2. Interroger directement la session média Windows (get_spotify.ps1)
                proc = subprocess.run(
                    ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", PS_SCRIPT_FILE],
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

                        cover_key = f"{artist}||{title}".lower()
                        pil_img = None
                        if cover_key != last_cover_key:
                            last_cover_key = cover_key
                            c_url = fetch_cover_url(artist, title)
                            pil_img = fetch_cover_pil(c_url, size=(100, 100))

                        self.root.after(
                            0,
                            lambda t=title, a=artist, al=album, pl=playing, im=pil_img: self.apply_song_to_ui(t, a, al, pl, im)
                        )
            except Exception:
                pass
            time.sleep(1.5)

    def animate_spectrum(self):
        try:
            self.spec_canvas.delete("all")
            w = max(280, self.spec_canvas.winfo_width())
            h = 22
            bar_count = 28
            bar_w = (w / bar_count) - 3
            t = time.time() * 10.0

            for i in range(bar_count):
                if self.playing:
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
        self.root.after(70, self.animate_spectrum)


if __name__ == "__main__":
    root = tk.Tk()
    app = StandaloneMusicWindow(root)
    root.mainloop()
