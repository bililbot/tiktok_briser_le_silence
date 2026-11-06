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
        from TikTokLive.events import ConnectEvent, CommentEvent, DisconnectEvent, GiftEvent, ShareEvent, FollowEvent
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
