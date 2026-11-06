# 📜 DOCUMENTATION TECHNIQUE — LIVE RÉGIE
**Version :** 2.2.2 (`rust_studio/Cargo.toml`, voir [CHANGELOG](../CHANGELOG.md)) • **Système :** Windows (x64) • **Langages :** Rust (régie egui 0.29) & HTML5/CSS3/ES6 (scènes) & Python (pont chat)
**Dernière mise à jour de la doc :** 06/11/2026 — alignée sur `rust_studio/src/main.rs`

> Ce document décrit le **fonctionnement interne**. Pour l'utilisation au quotidien, voir le [guide utilisateur](GUIDE_UTILISATEUR.md).

**Live Régie** est la régie compagnon de **TikTok LIVE Studio** (PC), développée pour le live « Et si… Briser le Silence ».

## 🗂️ 0. Carte du projet

| Chemin | Rôle |
| :--- | :--- |
| `rust_studio/src/main.rs` | Régie (egui), serveur HTTP local, état partagé, threads (audio, média, raccourcis, chrono, journal, tappeur) |
| `rust_studio/src/win_audio_media.rs` | WASAPI (VU-mètre), Windows Media Control (Spotify…), table des raccourcis `GLOBAL_HOTKEYS` |
| `rust_studio/src/music_box.rs` | Exe séparé `Musique_Live_TikTok` : fenêtre « musique en cours » capturée dans TikTok LIVE Studio |
| `rust_studio/build.rs` | Intègre l'icône et les métadonnées « Live Régie » dans les exe (`winresource`) |
| `stage_solution_b.html` / `stage_16_9.html` | Scènes 9:16 et 16:9 |
| `article_window.html` / `urgences_window.html` | Fenêtres dédiées presse et numéros d'urgence |
| `tiktok_chat_bridge.py` | Pont chat TikTok (TikTokLive 7.x), avec reconnexion automatique |
| `tools_python/scraper_article.py` | Extraction d'articles web (onglet Presse) |
| `tools/generer_logo.ps1` / `tools/creer_raccourci.ps1` | Génération du logo et du raccourci de lancement |
| `assets/` (`assets/logo/`) | Fonds de scène, tags, logo (`live_regie.ico`, `live_regie_256.png`, `live_regie_512.png`) |
| `Lancer_Studio_TikTok.bat` | Lanceur (relatif à son dossier ; ferme une instance orpheline puis démarre la régie) |
| `*.json` à la racine | Réglages : voir §14 |

---

## 🏗️ 1. Architecture Globale du Système

Le studio repose sur une architecture découplée, ultra-légère et résiliente, conçue pour minimiser la charge CPU/GPU pendant la diffusion en direct.

```mermaid
graph TD
    subgraph "Live Régie (Telecommande_Live_Rust.exe)"
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
| `GET /state` | État `FullLiveState` en JSON, **sans** `help_alerts` ni `journal` (privés) |
| `GET /stage` | Overlay 9:16 (`stage_solution_b.html`) |
| `GET /stage_16_9` | Overlay 16:9 (`stage_16_9.html`) |
| `GET /article` | Fenêtre Revue de presse (`article_window.html`) |
| `GET /urgences` | Fenêtre numéros d'urgence (`urgences_window.html`) |
| `GET /assets/...` | Images (PNG, WEBP, GIF, sinon `image/jpeg`) |
| `GET /video_stream` | Vidéo locale active, par blocs `206 Partial Content` |
| `GET /jingle_stream` | Fichier audio du jingle en cours |
| `POST /layout_save` | Mise en page d'une scène `{format, layout}` (format `9_16` ou `16_9`) (64 Ko max), stockée dans `stage_layouts` |
| `POST /chat_ingest` | Réception des événements du pont chat (`status`, `comment`, `gift`, `share`, `follow`) |

Tous les fichiers (overlays, images, JSON, `run_diag.log`) sont lus et écrits dans le **dossier du studio** (`get_app_dir()`). C'est le premier dossier contenant `stage_solution_b.html` parmi le dossier de l'exe et ses 3 parents, sinon le dossier courant. L'exe fonctionne donc où qu'il soit lancé, y compris depuis `rust_studio/target/release`.

La route `/assets/` refuse les chemins contenant `..`, `\` ou `:`, et sert PNG, JPEG, WEBP et GIF avec le bon type.

---

## 🎛️ 2. Structure des Onglets de la Régie

**Barre du haut** (`render_shield_bar` + `render_top_quick_bar`, toujours visible) :
- bouclier anti-raid ;
- nom et version (`env!("CARGO_PKG_VERSION")`) ;
- format 9:16 / 16:9 et mode mini ;
- **sélecteur unique des branches** (généré depuis `branches.json`) ;
- bascules urgences / TW / vidéo / jukebox.

**5 onglets** (`render_tab_selector`), rangés selon le moment d'utilisation. Chaque onglet est **composé de fonctions de section** ; la configuration est regroupée dans Réglages.

| Onglet | Fonction | Sections affichées |
| :--- | :--- | :--- |
| **🔴 Direct** | `render_tab_direct` | `render_debate_timer`, `section_branch_active`, `section_scene_texts`, `render_tab_soundboard` |
| **🖼️ Contenus** | `render_tab_contenus` | `section_video`, volet `render_tab_youtube` (préréglages), `section_annonce`, volet `render_tab_articles`, `section_music_detected` |
| **👥 Public** | `render_tab_public` | `render_tab_chat` (chat, épinglés, jukebox), `render_tab_sondages` (jauge, sondage) |
| **📝 Bilan** | `render_tab_bilan` | `render_help_alerts`, `render_journal_controls` |
| **⚙️ Réglages** | `render_tab_reglages` | volets repliables : `render_branch_editor`, `render_stage_layout_controls`, Sécurité (`section_emergency_display` + `section_trigger_warning`), `render_thanks_controls`, `render_tab_preferences` (comptes, auto-tappeur) |

> ℹ️ En 2.2.0, les anciens onglets Régie et Bannières ont été découpés en fonctions `section_*` (code déplacé tel quel). L'encart d'annonce, qui existait en double, a été supprimé de l'ancien onglet Régie.
> Barre du haut : voyant **● / ○ Chat** (`chat_status` contient « 🟢 ») et badge **🆘 N** (demandes non traitées, clic → Bilan).
> La configuration du bot et celle des numéros d'urgence s'ouvrent dans des fenêtres séparées (`bot_config.json`, `emergency_numbers.json`).

---

## 🎛️ 3. Branches Configurables (`branches.json`)

Chaque branche du live est entièrement configurable, sans recompilation. La configuration est stockée dans `branches.json` (struct `BranchConfig`), puis copiée dans `FullLiveState.branches`, que les scènes reçoivent via `/state`.

| Champ | Rôle |
| :--- | :--- |
| `id` | Identifiant interne (généré pour les nouvelles branches : `branche5`…) |
| `label` | Texte du bouton dans la régie, emoji compris |
| `color` / `accent` | Couleur principale (bouton et scènes) et couleur d'accent claire, au format `#rrggbb` |
| `tag` | Étiquette en bas de scène |
| `scene_title` | Titre d'en-tête de la scène |
| `topic` / `sub` | Sujet et sous-titre par défaut, appliqués à chaque bascule sur la branche |
| `box_mode` | Mode du bandeau : `combine`, `musique` ou `sujet` |
| `auto_emergency` | Affiche (ou masque) automatiquement les numéros d'urgence quand on passe sur cette branche |
| `bg_9_16` / `bg_16_9` | Fonds d'écran vertical et paysage |

* **Premier lancement** : si `branches.json` n'existe pas, il est créé avec les 4 branches historiques (VSS, Politique, Musique, Chill). Les fonds déjà choisis dans l'ancien gestionnaire à 8 décors (`custom_backgrounds`) sont repris.
* **Sélecteur unique** : la rangée de boutons en haut de la régie (et la version compacte du mode mini) est générée depuis la config, avec la couleur de chaque branche.
  * Les **3 premières** branches ont les raccourcis **F9, F10, F11** ; **F12** reste la vidéo.
  * Le bloc « BRANCHE ACTIVE » de l'onglet Direct ne sélectionne pas de branche. Il propose « ↺ Remettre les textes par défaut » et « 💾 Garder les textes actuels comme défaut ».
* **Éditeur** (onglet **⚙️ Réglages**, volet « 🎛️ Branches du live ») :
  * pour chaque branche : nom, deux couleurs, étiquette, titre, sujet, sous-titre, mode, urgences et deux fonds ;
  * boutons **⬆ Monter** (l'ordre décide des raccourcis F9 à F11), **🗑 Supprimer**, **➕ Ajouter une branche** et **🔄 Rétablir les 4 branches d'origine** ;
  * les modifications restent en **brouillon** jusqu'à **💾 Enregistrer** (ou **Annuler**).
* **Fonds** : l'image choisie est copiée dans `assets/custom_bg_{id}_{9_16|16_9}.{ext}` et référencée en `/assets/…?v={horodatage}`, ce qui force le rechargement dans OBS.
* **Scènes** : `branchCfg(st, br)` lit la config de la branche dans `st.branches`.
  * Si la branche n'y est pas, la scène se rabat sur les valeurs historiques (`BRANCH_CONFIG`) et sur les anciens `custom_backgrounds`.
  * Couleurs, étiquette et fond sont réappliqués au changement de branche **et** quand la config de la branche affichée est modifiée.

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

## ⌨️ 6. Raccourcis Clavier Globaux

Ils marchent même quand la régie est réduite ou n'a pas le focus.

| Touche | Action |
| :--- | :--- |
| **F1** | Jingle « alerte » (sirène) |
| **F2** | Épingle la dernière **question** du chat, ou retire l'épingle si une est affichée (inactif pendant le bouclier) |
| **F3** | Affiche / masque les numéros d'urgence |
| **F4** | Affiche / masque l'encart d'annonce libre (`custom_box_visible`) |
| **F8** | Bouclier anti-raid |
| **F9 / F10 / F11** | 1re / 2e / 3e branche de `branches.json` (ordre réglable dans Réglages → Branches du live) |
| **F12** | Affiche / masque la vidéo |

* **Implémentation** : la table `GLOBAL_HOTKEYS` (`win_audio_media.rs`) associe chaque touche à son action.
* Un thread dédié lit l'état des touches (`GetAsyncKeyState`) **toutes les 30 ms**, hors de l'interface.
* Une action est déclenchée **une seule fois par appui**, même si la touche reste enfoncée.
* Les actions sont exécutées par `apply_hotkey()` directement sur l'état partagé.

---

## 🛡️ 7. Bouclier Anti-Raid

* **Activation** : bouton rouge en haut de la régie, visible dans tous les onglets et en mode mini, ou **touche F8** (raccourci global, actif même quand la régie n'a pas le focus).
* **État** : `shield_active` et `shield_text` dans `FullLiveState`. Les deux champs sont marqués `#[serde(default)]`, donc les anciennes sauvegardes restent compatibles. Le bouclier est conservé en cas de redémarrage.
* **Effets côté régie (`/chat_ingest`)** :
  * les commentaires continuent d'arriver dans la liste de la régie, pour la modération ;
  * les déclencheurs `jingle` et `pin`, ainsi que l'épinglage automatique des questions, sont **gelés** ;
  * le déclencheur `emergency` reste actif.
* **Effets côté scènes** (`stage_solution_b.html` et `stage_16_9.html`) :
  * un écran plein cadre opaque (`#shieldOverlay`) masque tout le contenu ;
  * il affiche le message (`shield_text`, ou par défaut « Pause modération — Débat en cours de régulation ») et les 3 premiers numéros d'aide actifs ;
  * le commentaire épinglé est masqué.

---

## ⏱️ 8. Chrono de Débat

* **Commandes** : un bloc en tête de l'onglet **🔴 Direct**, avec :
  * un nom d'intervenant·e (`timer_label`) ;
  * les préréglages `▶ 1 min`, `▶ 3 min`, `▶ 5 min` ;
  * les boutons `+30 s`, `⏸ Pause` / `▶ Reprendre` et `⏹ Stop` ;
  * le temps restant affiché en direct.
* **État** (`#[serde(default)]`) : `timer_active`, `timer_running`, `timer_total_secs`, `timer_end_ts` (heure de fin, epoch s), `timer_remaining_secs` (figé en pause), `timer_gong_done`.
* **Thread dédié** (toutes les 200 ms) :
  * à 0, il déclenche le jingle synthé `gong` via le mécanisme `jingle_cmd_id` existant ;
  * 8 s après la fin, il masque le chrono.
* **Scènes** :
  * pastille `#debateTimer` en haut au centre, avec le nom, le décompte `mm:ss` et une barre de progression ;
  * jaune par défaut, orange à 30 s, rouge clignotant à 15 s, puis « ⏰ TEMPS ÉCOULÉ » à 0 ;
  * la pastille calcule le temps restant elle-même à partir de `timer_end_ts`, toutes les 200 ms, pour un décompte fluide ;
  * elle est masquée quand le bouclier est actif.

---

## 📝 9. Journal du Live

* **Ce qui est noté** :
  * des compteurs (`journal.comments`, `gifts` en nombre total de cadeaux, `shares`, `follows`), incrémentés à la réception dans `/chat_ingest` ;
  * une chronologie horodatée, tenue par **un seul thread observateur** : toutes les 500 ms, il compare l'état à la photo précédente et note les changements de branche, l'activation et la levée du bouclier, les lancements de chrono, les messages épinglés, le lancement et la fin des sondages (avec les votes) et les nouvelles demandes d'aide.
  * Une épingle affichée moins de 500 ms peut ne pas être notée.
* **Régie** : bloc « 📝 JOURNAL DU LIVE » dans l'onglet **📝 Bilan**.
  * Il affiche l'heure de début et les chiffres en cours.
  * Le bouton **🏁 Terminer le live & enregistrer le journal** écrit `journaux/live_AAAA-MM-JJ_HHhMM.md` puis repart sur un journal vierge.
  * Le bouton **📂 Dossier** ouvre le dossier des journaux.
* **Contenu du fichier** :
  * début, fin et durée ;
  * un tableau des chiffres ;
  * la chronologie ;
  * le dernier sondage ;
  * la liste des demandes d'aide **de ce live**, avec leur statut traité / à traiter.
* **Confidentialité** :
  * le journal est retiré de `/state`, comme les demandes d'aide ;
  * le dossier `journaux/` est exclu de Git (`.gitignore`), puisqu'il contient les demandes d'aide.

---

## 🆘 10. File Privée des Demandes d'Aide

* **Détection** : tout commentaire contenant un mot-clé d'un déclencheur `emergency` actif (`bot_config.json`, par exemple « aide, urgence, 119, 3919 ») est ajouté à `help_alerts` avec l'heure locale, le pseudo, le @handle et le texte.
* **Avant le filtre anti-insultes** : un message d'aide qui contient une insulte est masqué du chat, mais il est quand même noté, et il affiche quand même les numéros d'urgence.
* **Régie** : bloc repliable « 🆘 DEMANDES D'AIDE — N à traiter » dans l'onglet **📝 Bilan**, et badge **🆘 N** dans la barre du haut.
  * Il s'ouvre automatiquement s'il reste des demandes à traiter.
  * Pour chaque demande : un bouton 📋 pour copier le pseudo et un bouton ✅ Traité / ↩ À traiter.
  * Un bouton efface les demandes traitées.
* **Confidentialité** :
  * la file n'est **jamais** affichée sur les scènes ;
  * elle est **retirée de la réponse `/state`**, qui envoie `Access-Control-Allow-Origin: *` et serait sinon lisible par n'importe quelle page web ouverte sur la machine ;
  * elle n'est conservée que dans `live_state.json` et `studio_preferences.json`, deux fichiers exclus de Git.
* **Limite** : les 200 dernières demandes.

---

## 🔌 11. Pont Chat TikTok & Reconnexion Automatique

* **Lancement** : depuis la régie, avec `python <dossier du studio>/tiktok_chat_bridge.py <pseudo>`.
  * Le pont est lancé dans le dossier du studio, sans fenêtre.
  * La régie garde la main sur le processus : un nouveau clic sur « Connecter » **arrête l'ancien pont** avant d'en lancer un neuf, ce qui évite les commentaires en double.
  * La fermeture de la régie arrête aussi le pont.
  * **Au démarrage**, la régie relance le pont si `chat_handle` n'est pas vide (connexion automatique). Le voyant « ○ Chat » de la barre du haut est cliquable et relance aussi le pont.
  * **Pont orphelin** (régie plantée) : le PID du pont est écrit dans `chat_bridge.pid`. `kill_orphan_chat_bridge()` l'arrête avant tout nouveau lancement, uniquement si `tasklist` le désigne comme un processus Python (un PID peut être réutilisé).
* **Reconnexion** : à chaque tentative, le pont crée un client TikTokLive neuf.

| Situation | Comportement |
| :--- | :--- |
| Live ou réseau coupé | Nouvelle tentative après 5, 10, 20, 40, puis 60 s. Une connexion qui a tenu remet le délai à 5 s. |
| Compte pas (encore) en live | Nouvelle vérification toutes les 30 s (n'augmente pas le délai réseau) |
| Pseudo introuvable (`UserNotFoundError`) | Arrêt, avec un message dans la régie |
| Régie injoignable pendant 90 s | Arrêt du pont (évite les processus orphelins) |

* **Suivi** : le décompte « nouvelle tentative dans N s » s'affiche dans le statut du chat de la régie.

---

## 💜 12. Remerciements Automatiques (Cadeaux, Partages, Follows)

* **Pont chat** (`tiktok_chat_bridge.py`, TikTokLive 7.x) : il envoie à `POST /chat_ingest` des événements :
  * `gift` (`gift`, `count`) : un cadeau en série n'est envoyé qu'une fois, à la fin de la série (`event.streaking`), avec le total `repeat_count` ;
  * `share` et `follow`.
* **Régie** : bloc « 💜 Remerciements auto » dans **⚙️ Réglages** (volet Remerciements automatiques), avec une case d'activation (`thanks_enabled`, activée par défaut) et un bouton « 👁 Aperçu » qui envoie un faux cadeau.
* **État** : `thanks_seq` (compteur) et `thanks_queue` (les 20 derniers `ThanksItem`). Rien n'est ajouté pendant le bouclier anti-raid.
* **Scènes** :
  * une bulle `#thanksBubble` en bas à droite affiche « 💜 Merci @x pour 5 × Rose ! », « 🔁 Merci @x pour le partage ! » ou « ➕ Bienvenue @x, merci pour le follow ! » ;
  * les remerciements passent un par un, 3,5 s chacun, avec au plus 6 en attente (les plus anciens sont abandonnés en cas de rafale) ;
  * seuls les remerciements arrivés après l'ouverture de la scène sont joués, donc rien n'est rejoué quand OBS recharge la source ;
  * les pseudos sont insérés comme du texte brut, donc un pseudo contenant du HTML ne peut pas s'exécuter.

---

## 👆 13. Spécification Technique de l'Auto-Tappeur

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
* **Non implémenté.** Il n'existe aucune intégration WebView2 dans le code. Dans ce mode, la boucle reste en attente : aucun clic, aucun comptage.

### Comportement de la boucle
1. **Intervalle** : `1000 / clicks_per_second` ms, plus `0 à 36 ms` si le jitter est actif.
2. **Jitter spatial** : ±5 px en X et en Y autour du centre.
3. **Pause** : 2,4 s tous les 75 tapotements, si `random_pauses` est actif. La pause n'est pas aléatoire.
4. **Jauge de likes** : tous les 5 tapotements **réellement envoyés**, `like_goal.current_likes` augmente de 5. Si la fenêtre cible est introuvable, rien n'est compté et la boucle réessaie toutes les 500 ms.

> ⚠️ **Risque compte** : l'automatisation de tapotements pour gonfler l'engagement est contraire aux règles de TikTok. Elle peut entraîner une restriction du live ou du compte.

---

## 💾 14. Persistance & Fichiers de Configuration

### `live_state.json` et `studio_preferences.json`
* `save_state_json()` dépose l'état complet (`FullLiveState`) en mémoire. Un thread d'arrière-plan l'écrit **au plus une fois par seconde**, et seulement s'il a changé, dans `live_state.json` et `studio_preferences.json` du dossier du studio.
* À la fermeture de la régie (`on_exit`), la dernière sauvegarde en attente est écrite immédiatement.
* Les deux fichiers sont des copies identiques. Il n'existe pas de format de préférences séparé.
* **Au démarrage** (`load_preferences`) : lecture de `studio_preferences.json`, sinon de `live_state.json`. Seuls ces champs sont restaurés :
  * `branch`, `custom_topic`, `custom_sub` ;
  * `tiktok_host_account`, `tiktok_helper_account`, `chat_handle` ;
  * `custom_backgrounds`, `like_goal`, `poll`, `tapper_config`, `article` ;
  * `video_yt_autoplay`, `video_yt_loop`, `video_yt_controls`, `video_yt_mute`.

### Autres fichiers (lus/écrits dans le dossier du studio)
| Fichier | Contenu |
| :--- | :--- |
| `bot_config.json` | Filtre anti-insultes, déclencheurs du bot (`emergency`, `pin`, `jingle`) |
| `emergency_numbers.json` | Liste des numéros d'aide affichés |
| `jingles_library.json` | Bibliothèque de jingles |
| `video_library.json` | Dossier et liste des vidéos locales |
| `branches.json` | Branches du live (voir §3), créé au premier lancement |

**Dossiers et fichiers exclus de Git** (`.gitignore`) : `live_state.json`, `studio_preferences.json`, `journaux/`, les `.exe`, `*.lnk`, `videos/`, `rust_studio/target/`.

> ⚠️ Des copies anciennes de ces fichiers existent encore dans `rust_studio/`. Elles ne sont plus utilisées : seules celles de la racine comptent.

---

## 🚀 15. Procédure de Déploiement & Maintenance

1. **Vérification rapide** :
   ```bash
   cd rust_studio
   cargo check
   ```
2. **Compilation** (les deux exe, avec l'icône intégrée par `build.rs`) :
   ```bash
   cd rust_studio
   cargo build --release --bins
   ```
3. **Déploiement** : copier `rust_studio/target/release/Telecommande_Live_Rust.exe` et `Musique_Live_TikTok.exe` à la racine du projet.
   * La régie doit être fermée, sinon l'exe est verrouillé.
   * Garder les précédents dans `rust_studio/target/exe_precedents/` permet de revenir en arrière.
4. **Lancement** : raccourci `Live Régie.lnk` (ou `Lancer_Studio_TikTok.bat`).
   * Le `.bat` travaille dans son propre dossier (`%~dp0`).
   * Il ferme une éventuelle instance orpheline et les `pythonw.exe`, puis démarre la régie.
   * Une seule instance de la régie peut tourner (mutex Windows).
5. **Scènes dans TikTok LIVE Studio (ou OBS)** :
   * Scène 9:16 : `http://127.0.0.1:8765/stage`
   * Scène 16:9 : `http://127.0.0.1:8765/stage_16_9`
   * Presse : `http://127.0.0.1:8765/article`
   * Urgences : `http://127.0.0.1:8765/urgences`
   * La fenêtre musique se capture par son titre, `MUSIQUE_LIVE_TIKTOK` (à ne pas renommer).
   * Les scènes s'ouvrent dans une fenêtre Edge `--app` intitulée `STUDIO_LIVE_TIKTOK 9:16` (608×1080) ou `STUDIO_LIVE_TIKTOK 16:9`. Le changement de format ferme la scène ouverte (`close_all_stage_windows`, `WM_CLOSE` sur les titres qui contiennent `STUDIO_LIVE_TIKTOK`) puis ouvre l'autre. Edge ignore `--window-size` quand il tourne déjà en arrière-plan : `place_stage_window_when_ready()` attend donc la fenêtre (jusqu'à 10 s), puis la place avec `SetWindowPos` selon la zone utile de l'écran (`SPI_GETWORKAREA`, pixels réels). La 16:9 occupe 80 % de la zone, en haut à gauche, jamais en plein écran ; la 9:16 occupe 95 % de la hauteur.
   * Les fenêtres capturables (scènes, presse, urgences) sont lancées par `spawn_capture_window()` dans un **Edge dédié** (`--user-data-dir=edge_scene_profile`). Sans profil séparé, un Edge déjà lancé en arrière-plan (démarrage rapide, `--no-startup-window`) ignore `--disable-renderer-backgrounding`, `--disable-features=CalculateNativeWinOcclusion`, etc. : la page cachée derrière TikTok se met en veille et ne se met plus à jour.
   * Une fenêtre réduite ne peut pas être capturée. `stage_windows()` (`EnumWindows` + `IsIconic`) est vérifiée chaque seconde : si une scène est réduite, la régie affiche un bandeau, et le bouton « Afficher la scène » appelle `restore_stage_windows()` (`SW_SHOWNOACTIVATE`).
6. **Chat TikTok** : le pont `tiktok_chat_bridge.py <pseudo>` nécessite `pip install TikTokLive` (7.x).

**Méthode de test sans fermer la régie de production** : impossible, à cause du mutex d'instance unique et du port 8765. Il faut fermer la régie avant de lancer une version de test. Avant chaque test, sauvegarder `live_state.json` et `studio_preferences.json`.

---

## 🏷️ 16. Identité, Version & Publication

* **Nom** : Live Régie. Le titre de fenêtre `Live Régie — régie de live` est défini par la constante `APP_WINDOW_TITLE`, qui sert aussi à remettre la fenêtre au premier plan.
* **Logo** : console de régie à 3 curseurs (couleurs des branches) et point rouge « en direct ».
  * Il est généré en vectoriel par `tools/generer_logo.ps1` : PNG 256 et 512, et `.ico` 16 à 256 px (bitmaps 32 bits jusqu'à 128 px, PNG pour 256 px).
  * L'icône de fenêtre vient de `assets/logo/live_regie_256.png` (`include_bytes!`).
  * L'icône des exe et leurs métadonnées (nom, description, version) viennent de `build.rs` (`winresource`). Sans `rc.exe`, la compilation passe, mais sans icône, avec un avertissement.
  * Le raccourci `Live Régie.lnk` est créé par `tools/creer_raccourci.ps1` (`-Bureau` pour une copie sur le Bureau).
* **Versions** : `MAJEUR.MINEUR.CORRECTIF` dans `rust_studio/Cargo.toml`, affichée dans la régie. Le détail de chaque version est dans [CHANGELOG.md](../CHANGELOG.md).
* **Publication d'une version** :
  1. mettre à jour `Cargo.toml` et le `CHANGELOG.md` ;
  2. compiler et déployer (§15) ;
  3. committer, puis poser le tag `git tag -a vX.Y.Z -m "…"` ;
  4. pousser `main` et le tag (`git push origin main --tags`).
* **Git** : on développe sur une branche dédiée (`feat/…`, `fix/…`, `release/…`), puis on fusionne dans `main` en avance rapide. Dépôt privé : `github.com/bililbot/tiktok_briser_le_silence`.

---

## 🖼️ 17. Encarts Adaptatifs & Mise en Page des Scènes

* **Encarts déplaçables** (`.draggable-widget`) : position et taille en pourcentage de la scène (`stageRoot`).
* **Sauvegarde** (`saveLayout`) : `{leftPct, topPct, widthPct, heightPct}` par encart.
  * Elle est fusionnée avec la disposition précédente, pour que les encarts masqués gardent leur position.
  * Elle est écrite dans le `localStorage` (clé `bls_stage_layout_v4` en 9:16, `bls_stage_layout_16_9_v2` en 16:9) **et** envoyée à la régie par `POST /layout_save`.
* **Régie** : `FullLiveState.stage_layouts["9_16" | "16_9"]` est persisté avec l'état.
  * Au premier état reçu, la scène applique la disposition de la régie.
  * Si la régie n'en a pas encore, la scène lui transmet celle de son navigateur (migration).
  * `stage_layout_reset_id` est incrémenté par « ↺ Réinitialiser la mise en page ». Les scènes ouvertes appellent alors `resetWidgetPositions()`. Une scène rouverte plus tard compare cet identifiant à `localStorage[<clé>_reset]` et oublie sa disposition locale périmée.
* **Ajustement du contenu** (`fitWidget`), pour l'annonce, les urgences, l'avertissement et la question épinglée :
  * le contenu de la carte est enveloppé une fois dans `.fit-zoom`, qui reprend la disposition flex de la carte ;
  * quand l'encart a une hauteur explicite, une recherche dichotomique (12 itérations) trouve le plus grand `zoom` CSS, entre 0,2 et 4, pour lequel la carte ne déborde pas (`scrollHeight` / `scrollWidth` ≤ taille visible) ;
  * le résultat est mis en cache sur « taille du cadre + HTML du contenu » ;
  * le calcul est relancé par un `ResizeObserver` et à chaque état reçu (`applyAdaptive`) ;
  * au début d'un redimensionnement, la hauteur de ces encarts est figée, si bien qu'élargir le cadre agrandit le texte ;
  * la grille des urgences passe en `repeat(auto-fit, minmax(12em, 1fr))`.
* **Vidéo** : avec une hauteur explicite, `#videoWidget.fill-on` fait occuper à la zone vidéo toute la hauteur, sous le bandeau. `object-fit` suit `video_fit` (`contain` ou `cover`). L'iframe YouTube remplit l'encart et cadre elle-même la vidéo.
* **Réglages** (⚙️ Réglages → « Mise en page des scènes ») : `fit_content_auto`, `video_fit`, réinitialisation.