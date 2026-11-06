# -*- coding: utf-8 -*-
"""Récupère les sous-titres d'une vidéo YouTube pour Live Régie.

Usage : python youtube_subtitles.py <id_video> [langues]
  langues : codes séparés par des virgules, par ordre de préférence (défaut : "fr").

Écrit sur la sortie standard un JSON :
  {"success": true, "language": "Français", "code": "fr", "generated": false,
   "translated": false, "cues": [[debut_s, fin_s, "texte"], ...]}
ou {"success": false, "error": "..."}.

Ordre de recherche : sous-titres manuels dans une langue préférée, puis automatiques
dans une langue préférée, puis traduction automatique YouTube, puis n'importe quelle langue.
Si le résultat n'est pas dans la langue voulue, essai de Google Traduction (module deep-translator,
facultatif) ; sinon le JSON contient un champ "warning" et la régie réessaie plus tard.
Un résultat dans la bonne langue est gardé dans cache_sous_titres/<id>_<langue>.json.
Dépendances : pip install youtube-transcript-api (et deep-translator, facultatif)
"""
import json
import os
import sys
import time


def out(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False))
    sys.stdout.flush()


def candidates(transcripts, prefs):
    """Sous-titres à essayer, dans l'ordre : (transcript, traduit)."""
    found = []
    try:
        found.append((transcripts.find_manually_created_transcript(prefs), False))
    except Exception:
        pass
    try:
        found.append((transcripts.find_generated_transcript(prefs), False))
    except Exception:
        pass
    # Repli : les sous-titres automatiques sont dans la langue parlée de la vidéo, puis l'anglais, puis le reste
    def fallback_rank(t):
        if getattr(t, "is_generated", False):
            return 0
        if (getattr(t, "language_code", "") or "").lower().startswith("en"):
            return 1
        return 2
    available = sorted(list(transcripts), key=fallback_rank)
    if not found:
        # Traduction automatique YouTube (souvent refusée : on retombe alors sur la langue d'origine)
        for t in available:
            if getattr(t, "is_translatable", False):
                try:
                    found.append((t.translate(prefs[0]), True))
                except Exception:
                    pass
                break
    found.extend((t, False) for t in available)
    return found


def clean_cues(snippets):
    """Convertit les extraits en [debut, fin, texte] sans chevauchement (les sous-titres
    automatiques se chevauchent : la fin est ramenée au début de la ligne suivante)."""
    raw = []
    for s in snippets:
        text = (getattr(s, "text", "") or "").replace("\r", "").strip()
        if not text:
            continue
        start = float(getattr(s, "start", 0.0) or 0.0)
        dur = float(getattr(s, "duration", 0.0) or 0.0)
        raw.append([start, start + max(dur, 0.3), text])
    raw.sort(key=lambda c: c[0])
    for i in range(len(raw) - 1):
        if raw[i][1] > raw[i + 1][0]:
            raw[i][1] = max(raw[i][0] + 0.2, raw[i + 1][0])
    return [[round(a, 3), round(b, 3), t] for a, b, t in raw]


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass
    if len(sys.argv) < 2 or not sys.argv[1].strip():
        out({"success": False, "error": "Identifiant de vidéo manquant"})
        return
    video_id = sys.argv[1].strip()
    prefs = [l.strip() for l in (sys.argv[2] if len(sys.argv) > 2 else "fr").split(",") if l.strip()] or ["fr"]

    cached = cache_read(video_id, prefs)
    if cached:
        out(cached)
        return

    try:
        from youtube_transcript_api import YouTubeTranscriptApi
    except ImportError:
        out({"success": False, "error": "Module manquant : python -m pip install youtube-transcript-api"})
        return

    try:
        transcripts = YouTubeTranscriptApi().list(video_id)
    except Exception as e:
        name = type(e).__name__
        if name in ("TranscriptsDisabled", "NoTranscriptFound"):
            out({"success": False, "none": True, "error": "Pas de sous-titres pour cette vidéo"})
        elif name in ("VideoUnavailable", "InvalidVideoId"):
            out({"success": False, "error": "Vidéo introuvable ou privée"})
        elif name in ("RequestBlocked", "IpBlocked", "YouTubeRequestFailed"):
            out({"success": False, "error": "YouTube refuse la demande pour l'instant (réessaie plus tard ou mets le module à jour)"})
        else:
            out({"success": False, "error": f"{name} : {str(e).splitlines()[0][:120] if str(e) else ''}"})
        return

    tried = candidates(transcripts, prefs)
    if not tried:
        out({"success": False, "none": True, "error": "Pas de sous-titres pour cette vidéo"})
        return
    transcript, translated, cues, last_error = None, False, [], ""
    for cand, is_translated in tried[:4]:
        try:
            cues = clean_cues(cand.fetch())
        except Exception as e:
            last_error = type(e).__name__
            continue
        if cues:
            transcript, translated = cand, is_translated
            break
    if transcript is None:
        if last_error:
            out({"success": False, "error": f"Téléchargement refusé par YouTube ({last_error}), réessaie plus tard"})
        else:
            out({"success": False, "none": True, "error": "Sous-titres vides"})
        return
    result = {
        "success": True,
        "language": (getattr(transcript, "language", "") or "").replace(" (auto-generated)", ""),
        "code": getattr(transcript, "language_code", "") or "",
        "generated": bool(getattr(transcript, "is_generated", False)),
        "translated": translated,
        "cues": cues,
    }
    if not translated and not lang_matches(result["code"], prefs):
        # YouTube a refusé sa traduction : on essaie Google Traduction, sinon on prévient
        fr_cues = google_translate_cues(cues, prefs[0])
        if fr_cues:
            result.update({"cues": fr_cues, "translated": True, "translated_by": "Google Traduction",
                           "language": f"{prefs[0]} (traduit de : {result['language']})", "code": prefs[0]})
        else:
            result["warning"] = (f"{result['language']} au lieu de « {prefs[0]} » : la vidéo n'a pas de sous-titres "
                                 f"« {prefs[0]} » et la traduction est refusée pour l'instant. Nouvel essai automatique toutes les 5 min.")
    if "warning" not in result:
        cache_write(video_id, prefs, result)
    out(result)


# --- Cache : une traduction réussie est gardée, YouTube et Google la refusent souvent ensuite ---
CACHE_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "cache_sous_titres")


def cache_path(video_id, prefs):
    safe = "".join(c for c in video_id if c.isalnum() or c in "-_")
    return os.path.join(CACHE_DIR, f"{safe}_{prefs[0]}.json")


def cache_read(video_id, prefs):
    try:
        with open(cache_path(video_id, prefs), encoding="utf-8") as f:
            data = json.load(f)
        if data.get("success") and data.get("cues"):
            data["cached"] = True
            return data
    except Exception:
        pass
    return None


def cache_write(video_id, prefs, result):
    try:
        os.makedirs(CACHE_DIR, exist_ok=True)
        with open(cache_path(video_id, prefs), "w", encoding="utf-8") as f:
            json.dump(result, f, ensure_ascii=False)
    except Exception:
        pass


def lang_matches(code, prefs):
    base = (code or "").lower().split("-")[0]
    return any(base == p.lower().split("-")[0] for p in prefs)


def google_translate_cues(cues, target):
    """Traduit les lignes par paquets de ~4 500 caractères (module deep-translator, facultatif).
    Renvoie None si le module manque, si Google refuse ou si le découpage en lignes n'est pas conservé."""
    try:
        from deep_translator import GoogleTranslator
    except ImportError:
        return None
    translator = GoogleTranslator(source="auto", target=target)
    texts = [c[2].replace("\n", " ") for c in cues]
    out_texts, chunk = [], []

    def flush():
        if not chunk:
            return True
        res = translator.translate("\n".join(chunk))
        lines = (res or "").split("\n")
        if len(lines) != len(chunk):
            return False
        out_texts.extend(l.strip() for l in lines)
        chunk.clear()
        time.sleep(0.3)
        return True

    try:
        size = 0
        for t in texts:
            if size + len(t) + 1 > 4500 and not flush():
                return None
            if not chunk:
                size = 0
            chunk.append(t)
            size += len(t) + 1
        if not flush():
            return None
    except Exception:
        return None
    return [[c[0], c[1], t] for c, t in zip(cues, out_texts)]


if __name__ == "__main__":
    main()
