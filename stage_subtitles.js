// Sous-titres des vidéos diffusées par Live Régie (commun aux scènes 9:16 et 16:9, servi par /subs.js).
//
// - Les sous-titres de la vidéo en cours sont lus sur /subs_data quand `subtitles_id` change.
// - La position de lecture vient du lecteur local (<video>) ou du lecteur YouTube intégré :
//   l'iframe (enablejsapi=1) envoie ses « infoDelivery » (currentTime, playerState) après un message « listening ».
// - Modes (`video_display_mode`) : "video" | "video_subs" | "audio_subs" (image cachée, son conservé).
// La scène appelle updateSubtitles(st) à chaque état reçu de la régie.
(function () {
    const SUBS_URL = "http://127.0.0.1:8765/subs_data";

    // Styles de l'encart (communs aux deux scènes). La taille du texte suit la largeur de l'encart (cqw).
    const css = document.createElement("style");
    css.textContent = `
        #subtitleWidget { display: none; z-index: 27; container-type: inline-size; }
        #subtitleWidget { ${/9:16/.test(document.title) ? "left: 4%; top: 64%; width: 92%;" : "left: 10%; top: 80%; width: 80%;"} }
        #subtitleWidget .subs-card {
            background: rgba(8, 8, 14, 0.84); border: 1px solid rgba(255, 255, 255, 0.14);
            border-radius: 0.6em; padding: 0.35em 0.9em; text-align: center;
            box-shadow: 0 8px 24px rgba(0, 0, 0, 0.7); transition: opacity 0.2s ease;
        }
        #subtitleWidget.idle .subs-card { opacity: 0; }
        #subtitleWidget .subs-text {
            color: #ffffff; font-family: 'Inter', 'Segoe UI', sans-serif; font-weight: 800; line-height: 1.25;
            white-space: pre-line; font-size: calc(3.4cqw * var(--subs-scale, 1));
            text-shadow: 0 2px 4px #000, 0 0 2px #000;
        }
        #subtitleWidget .subs-listening {
            display: none; color: #c4b5fd; font-family: 'Inter', 'Segoe UI', sans-serif; font-weight: 800;
            letter-spacing: 0.5px; text-transform: uppercase; margin-bottom: 0.15em;
            font-size: calc(2cqw * var(--subs-scale, 1));
        }
        /* Son + sous-titres : la vidéo joue toujours, son image est invisible */
        #videoWidget.audio-only { opacity: 0 !important; pointer-events: none !important; }
    `;
    document.head.appendChild(css);
    let cues = [];             // [[debut, fin, texte], ...] triés
    let loadedId = null;
    let loading = false;
    let mode = "video";
    let active = false;        // vidéo affichée sur le live
    let source = "local";
    let videoTitle = "";
    let lastText = null;

    // --- Lecteur YouTube : position de lecture reçue par postMessage ---
    let ytTime = 0, ytTimeAt = 0, ytPlaying = false, ytLastMsg = 0;

    window.addEventListener("message", (ev) => {
        if (!/^https:\/\/www\.youtube(-nocookie)?\.com$/.test(ev.origin)) return;
        let d = ev.data;
        if (typeof d === "string") { try { d = JSON.parse(d); } catch (e) { return; } }
        if (!d || !d.info) return;
        ytLastMsg = performance.now();
        if (typeof d.info.currentTime === "number") { ytTime = d.info.currentTime; ytTimeAt = performance.now(); }
        if (typeof d.info.playerState === "number") {
            const wasPlaying = ytPlaying;
            ytPlaying = d.info.playerState === 1;
            if (wasPlaying !== ytPlaying) ytTimeAt = performance.now();
        }
    });

    function ytFrame() {
        const f = document.getElementById("ytIframePlayer");
        return f && f.src && f.contentWindow ? f : null;
    }

    // Demande à l'iframe d'envoyer sa position (à renouveler : l'iframe peut avoir été rechargée)
    setInterval(() => {
        const f = ytFrame();
        if (!f || performance.now() - ytLastMsg < 3000) return;
        f.contentWindow.postMessage(JSON.stringify({ event: "listening", id: "regie", channel: "widget" }), "*");
    }, 1000);

    // Commande au lecteur YouTube (utilisée aussi par les boutons lecture / pause / ±10 s de la régie)
    window.ytCommand = function (func, args) {
        const f = ytFrame();
        if (f) f.contentWindow.postMessage(JSON.stringify({ event: "command", func: func, args: args || [] }), "*");
    };
    window.ytIsPlaying = () => ytPlaying;
    window.ytCurrentTime = () => ytTime + (ytPlaying ? (performance.now() - ytTimeAt) / 1000 : 0);

    function currentTime() {
        if (source === "youtube") return window.ytCurrentTime();
        const v = document.getElementById("localVideoPlayer");
        return v ? v.currentTime : 0;
    }

    // Recherche dichotomique de la ligne à afficher
    function cueAt(t) {
        let lo = 0, hi = cues.length - 1;
        while (lo <= hi) {
            const mid = (lo + hi) >> 1;
            if (cues[mid][0] <= t) lo = mid + 1; else hi = mid - 1;
        }
        const c = cues[hi];
        return c && t < c[1] ? c[2] : "";
    }

    function loadCues(id) {
        if (loading) return;
        loading = true;
        fetch(SUBS_URL + "?t=" + Date.now(), { cache: "no-store" })
            .then(r => r.json())
            .then(j => { cues = Array.isArray(j.cues) ? j.cues : []; loadedId = j.id; lastText = null; })
            .catch(() => {})
            .finally(() => { loading = false; });
    }

    function render() {
        const w = document.getElementById("subtitleWidget");
        if (!w) return;
        const show = active && mode !== "video";
        w.style.display = show ? "block" : "none";
        if (!show) return;
        const text = cueAt(currentTime());
        if (text !== lastText) {
            lastText = text;
            const el = document.getElementById("subsText");
            el.textContent = text;
            el.classList.toggle("empty", !text);
        }
        // Entre deux phrases, l'encart s'efface (sauf en mode son seul, où « En écoute » reste affiché)
        w.classList.toggle("idle", !text && mode !== "audio_subs");
    }
    setInterval(render, 120);

    // Lecteur musique et barre du bas : affichés ou masqués depuis la régie (boutons Lecteur / Barre).
    // Appliqué seulement quand la régie change d'avis, pour laisser marcher le bouton « Barre Bas » de la scène.
    let lastHudOn = null, lastFooterOn = null;
    window.applySceneBars = function (st) {
        const hudOn = st.music_hud_visible !== false;
        const footerOn = st.footer_visible !== false;
        const hud = document.getElementById("musicWidget");
        const footer = document.getElementById("footerWidget");
        if (hud && hudOn !== lastHudOn) { lastHudOn = hudOn; hud.style.display = hudOn ? "" : "none"; }
        if (footer && footerOn !== lastFooterOn) { lastFooterOn = footerOn; footer.style.display = footerOn ? "block" : "none"; }
    };
    // Après « Réinitialiser positions », le choix de la régie est réappliqué au prochain état
    window.resetSceneBars = function () { lastHudOn = null; lastFooterOn = null; };

    window.updateSubtitles = function (st) {
        window.applySceneBars(st);
        mode = st.video_display_mode || "video";
        active = !!st.video_active;
        source = st.video_source_type === "youtube" ? "youtube" : "local";
        const id = st.subtitles_id || 0;
        if (id !== loadedId) loadCues(id);

        const w = document.getElementById("subtitleWidget");
        if (w) w.style.setProperty("--subs-scale", (st.subtitles_scale_pct || 100) / 100);

        // Son + sous-titres : la vidéo continue de jouer, seule son image est cachée
        const vw = document.getElementById("videoWidget");
        if (vw) vw.classList.toggle("audio-only", active && mode === "audio_subs");

        videoTitle = (st.video_title || "").replace(/^🔴\s*YOUTUBE\s*:\s*/i, "").trim();
        const listening = document.getElementById("subsListening");
        if (listening) {
            const on = active && mode === "audio_subs";
            listening.style.display = on ? "block" : "none";
            if (on) listening.textContent = "🔊 En écoute" + (videoTitle && !/^[\w-]{11}$/.test(videoTitle) ? " : " + videoTitle : "");
        }
    };
})();
