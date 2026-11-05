# 📜 DOCUMENTATION TECHNIQUE — STUDIO LIVE TIKTOK « BRISER LE SILENCE »
**Version :** 2.0.0 (`rust_studio/Cargo.toml`) • **Système :** Windows (x64) • **Langages :** Rust (Backend/Régie egui 0.29) & HTML5/CSS3/ES6 (Overlays OBS)
**Dernière mise à jour de la doc :** 06/11/2026 — alignée sur `rust_studio/src/main.rs`

---

## 🏗️ 1. Architecture Globale du Système

Le studio repose sur une architecture découplée, ultra-légère et résiliente, conçue pour minimiser la charge CPU/GPU pendant la diffusion en direct.

```mermaid
graph TD
    subgraph "Application Rust (Telecommande_Live_Rust.exe)"
        UI["Interface Régie egui"]
        HTTP["Serveur Local tiny_http<br>(127.0.0.1:8765, 8 threads)"]
        STATE["État Partagé en Mémoire<br>Arc<Mutex<FullLiveState>>"]
        WASAPI["Moteur Audio WASAPI<br>(Dual-Metering: Micro + Desktop)"]
        TAPPER["Moteur Auto-Tappeur<br>(PostMessageW uniquement)"]
        CONFIG["live_state.json + studio_preferences.json"]
    end

    subgraph "Logiciel de Stream (OBS Studio / TikTok Live Studio)"
        STAGE_9_16["Overlay Vertical 9:16<br>(stage_solution_b.html)"]
        STAGE_16_9["Overlay Paysage 16:9<br>(stage_16_9.html)"]
        ARTICLE_WIN["Fenêtre Dédiée Presse<br>(article_window.html)"]
        URGENCES_WIN["Fenêtre Numéros Urgences<br>(urgences_window.html)"]
    end

    subgraph "Sources Externes & Réseau"
        TIKTOK_API["Chat TikTok Live<br>(tiktok_chat_bridge.py / TikTokLive)"]
        SPOTIFY_API["Windows Media Control<br>(GSMTC / Spotify)"]
        YT_EMBED["YouTube no-cookie<br>(Lecteur Iframe)"]
    end

    WASAPI --> STATE
    SPOTIFY_API --> STATE
    TIKTOK_API -->|POST /chat_ingest| HTTP
    UI -->|Édition directe| STATE
    STATE -->|JSON /state| HTTP
    HTTP -->|polling 250 ms| STAGE_9_16
    HTTP -->|polling 250 ms| STAGE_16_9
    HTTP --> ARTICLE_WIN
    HTTP --> URGENCES_WIN
    CONFIG <-->|Lecture au démarrage / Écriture à chaque changement| STATE
    TAPPER -->|PostMessageW| TIKTOK_API
    YT_EMBED --> STAGE_9_16
    YT_EMBED --> STAGE_16_9
```

### Routes du serveur HTTP (`handle_http_request`)
| Route | Rôle |
| :--- | :--- |
| `GET /state` | État complet `FullLiveState` en JSON |
| `GET /stage` | Overlay 9:16 (`stage_solution_b.html`) |
| `GET /stage_16_9` | Overlay 16:9 (`stage_16_9.html`) |
| `GET /article` | Fenêtre Revue de presse (`article_window.html`) |
| `GET /urgences` | Fenêtre numéros d'urgence (`urgences_window.html`) |
| `GET /assets/...` | Images (PNG, sinon servies en `image/jpeg`) |
| `GET /video_stream` | Vidéo locale active, par blocs `206 Partial Content` |
| `GET /jingle_stream` | Fichier audio du jingle en cours |
| `POST /chat_ingest` | Réception des événements du pont chat (`status` / `comment`) |

Les fichiers sont résolus (`resolve_app_file`) dans cet ordre : dossier courant → dossier de l'exe → `C:\laragon\www\tiktok_briser_le_silence` (chemin codé en dur).

---

## 🎛️ 2. Structure des Onglets de la Régie Rust

La barre d'onglets affiche **6 onglets** sur deux rangées :

| N° | Onglet (libellé) | Fonctionnalités Clés |
| :--- | :--- | :--- |
| **1** | **[ Régie ]** | • Sélecteur des **4 branches** : `VSS`, `Politique`, `Musique`, `☕ Chill`<br>• Titre du Live et Sous-titre personnalisés<br>• Bandeau d'annonce déroulant<br>• Bascule des formats cibles (9:16 ou 16:9) |
| **2** | **[ YouTube ]** | • Champ de saisie d'URL YouTube universel (Shorts, Watch, Embed, ID)<br>• Options : `Autoplay`, `Loop`, `Controls`, `Mute`, `Anti-détection`<br>• Sélecteur de vidéos locales MP4/MKV |
| **3** | **[ Presse ]** | • Scraping d'articles web depuis une URL (`tools_python/scraper_article.py`)<br>• Auto-scroll avec réglage de vitesse<br>• Personnalisation typographique, couleurs et badges |
| **4** | **🗳️ Sondages** | • Sondages à chaud (choix via le chat)<br>• Jauge d'objectif Likes (`like_goal`)<br>• Modes chanson : `off` / `queue` / `vote` |
| **5** | **🎵 Sons** | • VU-mètre WASAPI double écoute<br>• Lecteur d'état Spotify (Titre, Artiste, Pochette)<br>• Soundboard / jingles (`jingles_library.json`) |
| **6** | **[ Préférences ]** | • Auto-Tappeur (voir §6)<br>• Compte TikTok hôte & compte secondaire<br>• Gestionnaire des 8 fonds d'écran |

> ℹ️ L'enum `TelecommandeTab` contient aussi `Chat` et `Bannieres`, mais ils **n'apparaissent pas** dans la barre d'onglets.
> La configuration du bot chat et des numéros d'urgence s'ouvre dans des fenêtres séparées (`bot_config.json`, `emergency_numbers.json`).

---

## 🖼️ 3. Gestionnaire des 8 Décors / Images de Fond

Les scènes s'adaptent instantanément à la branche active sans recompilation. Les 8 emplacements sont stockés dans `FullLiveState.custom_backgrounds` (struct `CustomBackgrounds`).

### Valeurs par défaut
| Emplacement | Fichier par défaut |
| :--- | :--- |
| `vss_9_16` | `/assets/bg_branche1_vss.jpg` |
| `politique_9_16` | `/assets/bg_branche2_politique.jpg` |
| `musique_9_16` | `/assets/bg_branche3_musique.jpg` |
| `chill_9_16` | `/assets/bg_branche4_chill.jpg` |
| `vss_16_9` | `/assets/bg_branche1_vss_wide16_9.jpg` |
| `politique_16_9` | `/assets/bg_branche2_politique_wide16_9.jpg` |
| `musique_16_9` | `/assets/bg_branche3_musique_wide16_9.jpg` |
| `chill_16_9` | `/assets/bg_branche4_chill_wide16_9.jpg` |

### Remplacement d'un fond (bouton « 📂 Parcourir... »)
1. L'image choisie (JPG, PNG, WEBP) est **copiée** vers `C:\laragon\www\tiktok_briser_le_silence\assets\custom_bg_{emplacement}.{ext}` (chemin codé en dur).
2. L'emplacement prend la valeur `/assets/custom_bg_{emplacement}.{ext}?v={timestamp}` (le `?v=` force le rechargement dans OBS).
3. L'image est servie par la route générique `/assets/` : il n'existe **pas** de route `/assets/custom_bg/...` dédiée.

> ⚠️ La route `/assets/` sert les `.webp` avec le type `image/jpeg`.

---

## 🎙️ 4. Architecture Audio WASAPI Dual-Metering

Implémentée dans `rust_studio/src/win_audio_media.rs` via `IAudioMeterInformation` :

* **`eRender`** (sortie casque / haut-parleurs) : musique Spotify, vidéos YouTube, sons Windows, alertes.
* **`eCapture`** (micro du streamer) avec un gain de `x1.4`.
* **Calcul de crête** :
  $$\text{Peak} = \max\left(\text{Peak}_{\text{Render}}, \, \min(1.4 \times \text{Peak}_{\text{Capture}},\, 1)\right)$$
* **Cadence** : thread dédié, mesure toutes les 25 ms, `audio_peak` publié dans l'état partagé toutes les 50 ms. L'interface n'est jamais bloquée.

---

## 📺 5. Module d'Intégration YouTube

Le lecteur YouTube des overlays (`stage_solution_b.html` et `stage_16_9.html`) :

* **Domaine** : `https://www.youtube-nocookie.com/embed/{id}`.
* **Paramètres** pilotés depuis la régie (`video_yt_autoplay`, `video_yt_loop`, `video_yt_controls`, `video_yt_mute`) :
  * `autoplay=1`, `loop=1&playlist={id}`, `controls=0|1`, `mute=0|1`, `modestbranding=1&rel=0`.
* **Filtre « Anti-détection »** (`video_anti_detect`) : couche CSS de recadrage/zoom (`#antiDetectOverlay`).

> ⚠️ Diffuser une vidéo dont on n'a pas les droits reste une infraction aux règles TikTok, que le filtre soit actif ou non.

---

## 👆 6. Spécification Technique de l'Auto-Tappeur

Configuration : `FullLiveState.tapper_config` (struct `TapperConfig`).

| Champ | Défaut | Rôle |
| :--- | :--- | :--- |
| `enabled` | `false` | Active la boucle |
| `mode` | `"post_message"` | `post_message` ou `webview` |
| `clicks_per_second` | `4` | Curseur 1 à 12 dans l'interface (borné à 15 en interne) |
| `jitter_enabled` | — | Dispersion du point de clic et de l'intervalle |
| `random_pauses` | — | Pause périodique |
| `target_window_title` | `"TikTok"` | Fenêtre ciblée (titre exact, sinon sous-chaîne) |

### Mode `post_message` (seul mode réellement implémenté)
* Envoie `WM_LBUTTONDOWN` puis, 20 ms plus tard, `WM_LBUTTONUP` au **centre** de la zone client de la fenêtre cible, via `PostMessageW`.
* Le curseur de la souris n'est pas déplacé.

### Mode `webview`
* **Non implémenté.** Il n'existe aucune intégration WebView2 dans le code. Ce mode ne fait qu'incrémenter le compteur interne, sans aucun clic réel.

### Comportement de la boucle
1. **Intervalle** : `1000 / clicks_per_second` ms, plus `0 à 36 ms` si le jitter est actif.
2. **Jitter spatial** : ±5 px en X et en Y autour du centre.
3. **Pause** : 2,4 s tous les 75 tapotements, si `random_pauses` est actif. La pause n'est pas aléatoire.
4. **Jauge de likes** : tous les 5 tapotements comptés, `like_goal.current_likes` augmente de 5. En mode `webview`, la jauge monte donc sans aucun clic réel.

> ⚠️ **Risque compte** : l'automatisation de tapotements pour gonfler l'engagement est contraire aux règles de TikTok. Elle peut entraîner une restriction du live ou du compte.

---

## 💾 7. Persistance & Fichiers de Configuration

### `live_state.json` et `studio_preferences.json`
* `save_state_json()` écrit **le même contenu** (l'intégralité de `FullLiveState`) dans **4 fichiers** à chaque changement :
  * `live_state.json` et `studio_preferences.json` dans le dossier de l'application (`get_app_dir()`) ;
  * les mêmes dans le dossier courant (d'où les copies dans `rust_studio/` quand on lance depuis ce dossier).
* Les deux fichiers sont donc des copies identiques. Il n'existe pas de format de préférences séparé.
* **Au démarrage** (`load_preferences`) : lecture de `studio_preferences.json`, sinon de `live_state.json`. Seuls ces champs sont restaurés :
  * `branch`, `custom_topic`, `custom_sub` ;
  * `tiktok_host_account`, `tiktok_helper_account`, `chat_handle` ;
  * `custom_backgrounds`, `like_goal`, `poll`, `tapper_config`, `article` ;
  * `video_yt_autoplay`, `video_yt_loop`, `video_yt_controls`, `video_yt_mute`.

### Autres fichiers (lus/écrits dans le dossier courant)
| Fichier | Contenu |
| :--- | :--- |
| `bot_config.json` | Filtre anti-insultes, déclencheurs du bot (`emergency`, `pin`, `jingle`) |
| `emergency_numbers.json` | Liste des numéros d'aide affichés |
| `jingles_library.json` | Bibliothèque de jingles |
| `video_library.json` | Dossier et liste des vidéos locales |

> ⚠️ Ces fichiers existent en double (racine et `rust_studio/`) avec des contenus différents. L'exe utilise ceux du **dossier courant**, c'est-à-dire la racine quand il est lancé via `Lancer_Studio_TikTok.bat`.

---

## 🚀 8. Procédure de Déploiement & Maintenance

1. **Vérification rapide** :
   ```bash
   cd rust_studio
   cargo check
   ```
2. **Compilation Native** :
   ```bash
   cd rust_studio
   cargo build --release --bin Telecommande_Live_Rust
   cargo build --release --bin Musique_Live_TikTok
   ```
3. **Déploiement Exécutable** :
   Copier `rust_studio/target/release/Telecommande_Live_Rust.exe` (et `Musique_Live_TikTok.exe`) à la racine de `C:\laragon\www\tiktok_briser_le_silence\`.
4. **Lancement** : `Lancer_Studio_TikTok.bat`.
   * Il ferme une éventuelle instance précédente et les `pythonw.exe`.
   * Il démarre l'exe depuis la racine.
   * Une seule instance de la régie peut tourner (mutex Windows).
5. **Overlays dans OBS / TikTok Live Studio** :
   * Scène 16:9 : `http://127.0.0.1:8765/stage_16_9`
   * Scène 9:16 : `http://127.0.0.1:8765/stage`
   * Presse : `http://127.0.0.1:8765/article`
   * Urgences : `http://127.0.0.1:8765/urgences`
6. **Chat TikTok** : le pont `tiktok_chat_bridge.py <pseudo>` nécessite `pip install TikTokLive`.
