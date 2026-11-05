# -*- coding: utf-8 -*-
"""
Pont de réception temps réel des commentaires TikTok LIVE -> Télécommande Rust (127.0.0.1:8765)
Usage : python tiktok_chat_bridge.py <pseudo_ou_url>
"""

import sys
import json
import time
import urllib.request
import urllib.parse
import re

RUST_SERVER = "http://127.0.0.1:8765"

def send_event_to_rust(payload: dict):
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
    except Exception:
        pass

def set_bridge_status(status: str):
    send_event_to_rust({"type": "status", "status": status})

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

def main():
    if len(sys.argv) < 2:
        set_bridge_status("❌ Pseudo manquant")
        return

    raw_target = sys.argv[1]
    unique_id = resolve_unique_id(raw_target)
    set_bridge_status(f"⏳ Connexion au Live de @{unique_id}...")

    try:
        from TikTokLive import TikTokLiveClient
        from TikTokLive.events import ConnectEvent, CommentEvent, DisconnectEvent, LikeEvent
    except ImportError:
        set_bridge_status("❌ Module TikTokLive non installé (pip install TikTokLive)")
        return

    client = TikTokLiveClient(unique_id=f"@{unique_id}")

    @client.on(ConnectEvent)
    async def on_connect(event: ConnectEvent):
        set_bridge_status(f"🟢 Connecté en direct au Live (@{client.unique_id}) !")

    @client.on(DisconnectEvent)
    async def on_disconnect(event: DisconnectEvent):
        set_bridge_status(f"🟠 Déconnecté du Live (@{unique_id})")

    @client.on(CommentEvent)
    async def on_comment(event: CommentEvent):
        try:
            user_nick = getattr(event.user, "nickname", "") or getattr(event.user, "unique_id", "Spectateur")
            user_id = getattr(event.user, "unique_id", "")
            avatar_url = ""
            try:
                if hasattr(event.user, "avatar_thumb") and event.user.avatar_thumb and event.user.avatar_thumb.m_urls:
                    avatar_url = event.user.avatar_thumb.m_urls[0]
            except Exception:
                avatar_url = ""

            comment_text = getattr(event, "comment", "") or ""
            if comment_text.strip():
                send_event_to_rust({
                    "type": "comment",
                    "author": user_nick,
                    "handle": f"@{user_id}" if user_id else "",
                    "text": comment_text.strip(),
                    "avatar": avatar_url,
                    "timestamp": int(time.time())
                })
        except Exception:
            pass

    try:
        client.run()
    except Exception as e:
        err_msg = str(e)
        if "LIVE_NOT_FOUND" in err_msg or "offline" in err_msg.lower() or "not live" in err_msg.lower():
            set_bridge_status(f"⚪ @{unique_id} n'est pas en Live actuellement (ou vérifie l'@identifiant exact)")
        else:
            set_bridge_status(f"⚠️ Info connexion (@{unique_id}) : {err_msg[:90]}")

if __name__ == "__main__":
    main()
