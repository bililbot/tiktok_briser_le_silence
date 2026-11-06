# -*- coding: utf-8 -*-
"""
Pont de réception temps réel des commentaires TikTok LIVE -> Télécommande Rust (127.0.0.1:8765)
Usage : python tiktok_chat_bridge.py <pseudo_ou_url>

Reconnexion automatique :
- coupure du live / du réseau : nouvelle tentative après 5, 10, 20, 40 puis 60 s
- compte pas (encore) en live : nouvelle vérification toutes les 30 s
- pseudo introuvable : arrêt (inutile de réessayer)
- régie fermée (plus de réponse pendant 90 s) : arrêt du pont
"""

import sys
import json
import time
import urllib.request
import re

RUST_SERVER = "http://127.0.0.1:8765"
RETRY_DELAYS = [5, 10, 20, 40, 60]
OFFLINE_DELAY = 30
REGIE_TIMEOUT = 90

last_regie_ok = time.time()


def send_event_to_rust(payload: dict) -> bool:
    global last_regie_ok
    try:
        data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        req = urllib.request.Request(
            f"{RUST_SERVER}/chat_ingest",
            data=data,
            headers={"Content-Type": "application/json; charset=utf-8"},
            method="POST"
        )
        with urllib.request.urlopen(req, timeout=1.5) as _:
            pass
        last_regie_ok = time.time()
        return True
    except Exception:
        return False


def set_bridge_status(status: str):
    send_event_to_rust({"type": "status", "status": status})


def regie_is_gone() -> bool:
    return time.time() - last_regie_ok > REGIE_TIMEOUT


def resolve_unique_id(raw_input: str) -> str:
    s = raw_input.strip()
    if not s:
        return ""
    # Si l'utilisateur colle un lien https://www.tiktok.com/@pseudo/live
    m = re.search(r"tiktok\.com/@([A-Za-z0-9_.\-]+)", s)
    if m:
        return m.group(1)
    if s.startswith("@"):
        s = s[1:].strip()
    return s


def user_payload(user) -> dict:
    nick = getattr(user, "nickname", "") or getattr(user, "unique_id", "") or "Spectateur"
    uid = getattr(user, "unique_id", "")
    avatar_url = ""
    try:
        if user.avatar_thumb and user.avatar_thumb.m_urls:
            avatar_url = user.avatar_thumb.m_urls[0]
    except Exception:
        avatar_url = ""
    return {"author": nick, "handle": f"@{uid}" if uid else "", "avatar": avatar_url}


def build_client(unique_id: str, session: dict):
    """Crée un client TikTokLive neuf (un client déconnecté ne se réutilise pas) avec tous les handlers."""
    from TikTokLive import TikTokLiveClient
    from TikTokLive.events import ConnectEvent, CommentEvent, DisconnectEvent, GiftEvent, ShareEvent, FollowEvent

    client = TikTokLiveClient(unique_id=f"@{unique_id}")

    @client.on(ConnectEvent)
    async def on_connect(event: ConnectEvent):
        session["connected"] = True
        set_bridge_status(f"🟢 Connecté en direct au Live (@{client.unique_id}) !")

    @client.on(DisconnectEvent)
    async def on_disconnect(event: DisconnectEvent):
        set_bridge_status(f"🟠 Déconnecté du Live (@{unique_id})")

    @client.on(CommentEvent)
    async def on_comment(event: CommentEvent):
        try:
            comment_text = getattr(event, "comment", "") or ""
            if comment_text.strip():
                payload = user_payload(event.user)
                payload.update({"type": "comment", "text": comment_text.strip(), "timestamp": int(time.time())})
                send_event_to_rust(payload)
        except Exception:
            pass

    @client.on(GiftEvent)
    async def on_gift(event: GiftEvent):
        try:
            # Cadeau en série (combo) : on attend le dernier événement qui porte le total
            if event.streaking:
                return
            payload = user_payload(event.user)
            payload.update({
                "type": "gift",
                "gift": getattr(event.gift, "name", "") or "cadeau",
                "count": max(1, int(getattr(event, "repeat_count", 1) or 1)),
            })
            send_event_to_rust(payload)
        except Exception:
            pass

    @client.on(ShareEvent)
    async def on_share(event: ShareEvent):
        try:
            payload = user_payload(event.user)
            payload["type"] = "share"
            send_event_to_rust(payload)
        except Exception:
            pass

    @client.on(FollowEvent)
    async def on_follow(event: FollowEvent):
        try:
            payload = user_payload(event.user)
            payload["type"] = "follow"
            send_event_to_rust(payload)
        except Exception:
            pass

    return client


def wait_before_retry(delay: int, message: str) -> bool:
    """Décompte affiché dans la régie. Retourne False si la régie a disparu entre-temps."""
    for remaining in range(delay, 0, -1):
        if remaining == delay or remaining % 5 == 0:
            set_bridge_status(f"{message} — nouvelle tentative dans {remaining} s")
        if regie_is_gone():
            return False
        time.sleep(1)
    return True


def main():
    if len(sys.argv) < 2:
        set_bridge_status("❌ Pseudo manquant")
        return

    unique_id = resolve_unique_id(sys.argv[1])

    try:
        from TikTokLive.client.errors import UserOfflineError, UserNotFoundError
    except ImportError:
        set_bridge_status("❌ Module TikTokLive non installé (pip install TikTokLive)")
        return

    failures = 0
    while not regie_is_gone():
        set_bridge_status(f"⏳ Connexion au Live de @{unique_id}...")
        session = {"connected": False}
        try:
            build_client(unique_id, session).run()
            reason = "🟠 Live coupé"
            delay = None
        except UserNotFoundError:
            set_bridge_status(f"❌ Compte @{unique_id} introuvable — vérifie l'identifiant exact")
            return
        except UserOfflineError:
            reason = f"⚪ @{unique_id} n'est pas en Live"
            delay = OFFLINE_DELAY
        except Exception as e:
            err_msg = str(e)
            if "LIVE_NOT_FOUND" in err_msg or "offline" in err_msg.lower() or "not live" in err_msg.lower():
                reason = f"⚪ @{unique_id} n'est pas en Live"
                delay = OFFLINE_DELAY
            else:
                reason = f"⚠️ Connexion perdue ({err_msg[:60]})"
                delay = None

        # Une connexion qui a vraiment tenu remet le compteur d'échecs à zéro ;
        # « pas en live » a son propre délai fixe et ne compte pas comme un échec réseau
        if session["connected"]:
            failures = 0
        elif delay is None:
            failures += 1
        if delay is None:
            delay = RETRY_DELAYS[min(max(failures - 1, 0), len(RETRY_DELAYS) - 1)]
        if not wait_before_retry(delay, reason):
            break


if __name__ == "__main__":
    main()
