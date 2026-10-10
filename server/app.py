"""Reference StudHub API. Merge webhook handler into the existing bot; one webhook per bot."""

import hashlib, json, os, re, secrets, sqlite3, threading, time
from collections import defaultdict, deque
from pathlib import Path
import requests
from fastapi import FastAPI, Header, HTTPException, Request
from pydantic import BaseModel, Field


def hash_secret(value):
    return hashlib.sha256(value.encode()).hexdigest()


class Poll(BaseModel):
    session_id: str = Field(max_length=64)
    poll_secret: str = Field(max_length=128)


class Nick(BaseModel):
    nickname: str = Field(pattern=r"^[A-Za-z0-9_]{3,16}$")


def create_app(
    db_path=None, bot_name=None, bot_token=None, webhook_secret=None, sender=None
):
    app = FastAPI(title="StudHub API", version="0.3.0")
    db = sqlite3.connect(
        db_path or os.getenv("STUDHUB_DB", "studhub.sqlite3"),
        check_same_thread=False,
        isolation_level=None,
    )
    db.row_factory = sqlite3.Row
    db.execute("PRAGMA journal_mode=WAL")
    db.executescript("""
    CREATE TABLE IF NOT EXISTS profiles(telegram_id INTEGER PRIMARY KEY, telegram_username TEXT, nickname TEXT COLLATE NOCASE UNIQUE, university TEXT, mode TEXT, clan TEXT, rating INTEGER);
    CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY, poll_hash TEXT NOT NULL, expires REAL NOT NULL, telegram_id INTEGER, approved INTEGER NOT NULL DEFAULT 0, used INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE IF NOT EXISTS tokens(hash TEXT PRIMARY KEY, telegram_id INTEGER NOT NULL, expires REAL NOT NULL);
    CREATE TABLE IF NOT EXISTS updates(id INTEGER PRIMARY KEY);
    """)
    lock = threading.RLock()
    name = bot_name or os.getenv("TELEGRAM_BOT_NAME", "")
    token = bot_token or os.getenv("TELEGRAM_BOT_TOKEN", "")
    secret = webhook_secret or os.getenv("TELEGRAM_WEBHOOK_SECRET", "")
    buckets = defaultdict(deque)
    rate_lock = threading.Lock()

    @app.middleware("http")
    async def rate_limit(request: Request, call_next):
        from starlette.responses import JSONResponse

        key = (request.client.host, request.url.path)
        now = time.monotonic()
        with rate_lock:
            bucket = buckets[key]
            while bucket and bucket[0] < now - 60:
                bucket.popleft()
            if len(bucket) >= 90:
                return JSONResponse({"detail": "Too many requests"}, status_code=429)
            bucket.append(now)
            if len(buckets) > 10000:
                for old in list(buckets):
                    if not buckets[old] or buckets[old][-1] < now - 60:
                        del buckets[old]
        return await call_next(request)

    def send(method, payload):
        if sender:
            return sender(method, payload)
        if not token:
            raise HTTPException(503, "Bot token not configured")
        r = requests.post(
            f"https://api.telegram.org/bot{token}/{method}", json=payload, timeout=15
        )
        if not r.ok or not r.json().get("ok"):
            raise HTTPException(502, "Telegram delivery failed")
        return r.json()

    def profile(tg):
        row = db.execute("SELECT * FROM profiles WHERE telegram_id=?", (tg,)).fetchone()
        if not row:
            raise HTTPException(404, "Profile not found")
        return dict(row)

    def user(authorization):
        if not authorization or not authorization.startswith("Bearer "):
            raise HTTPException(401, "Login required")
        row = db.execute(
            "SELECT telegram_id FROM tokens WHERE hash=? AND expires>?",
            (hash_secret(authorization[7:]), time.time()),
        ).fetchone()
        if not row:
            raise HTTPException(401, "Session expired")
        return row[0]

    @app.post("/auth/telegram/session")
    def session():
        if not re.fullmatch("[A-Za-z0-9_]{5,32}", name) or not secret:
            raise HTTPException(503, "Telegram configuration required")
        sid, poll = secrets.token_urlsafe(24), secrets.token_urlsafe(32)
        with lock:
            db.execute("DELETE FROM sessions WHERE expires<?", (time.time() - 600,))
            db.execute("DELETE FROM tokens WHERE expires<?", (time.time(),))
            db.execute(
                "INSERT INTO sessions(id,poll_hash,expires) VALUES(?,?,?)",
                (sid, hash_secret(poll), time.time() + 300),
            )
        return {
            "session_id": sid,
            "poll_secret": poll,
            "expires_in": 300,
            "bot_url": f"https://t.me/{name}?start=launcher_{sid}",
        }

    @app.post("/auth/telegram/poll")
    def poll(body: Poll):
        with lock:
            db.execute("BEGIN IMMEDIATE")
            try:
                row = db.execute(
                    "SELECT * FROM sessions WHERE id=?", (body.session_id,)
                ).fetchone()
                if not row or not secrets.compare_digest(
                    row["poll_hash"], hash_secret(body.poll_secret)
                ):
                    raise HTTPException(401, "Invalid session")
                if row["used"] or row["expires"] <= time.time():
                    raise HTTPException(410, "Session expired or consumed")
                if not row["approved"]:
                    db.execute("COMMIT")
                    return {"status": "pending"}
                bearer = secrets.token_urlsafe(48)
                db.execute("UPDATE sessions SET used=1 WHERE id=?", (body.session_id,))
                db.execute(
                    "INSERT INTO tokens VALUES(?,?,?)",
                    (hash_secret(bearer), row["telegram_id"], time.time() + 30 * 86400),
                )
                answer = {
                    "status": "approved",
                    "token": bearer,
                    "profile": profile(row["telegram_id"]),
                }
                db.execute("COMMIT")
                return answer
            except Exception:
                db.execute("ROLLBACK")
                raise

    @app.post("/telegram/webhook")
    async def webhook(
        request: Request, x_telegram_bot_api_secret_token: str = Header(default="")
    ):
        if not secret or not secrets.compare_digest(
            secret, x_telegram_bot_api_secret_token
        ):
            raise HTTPException(403, "Invalid webhook secret")
        body = await request.body()
        if len(body) > 1024 * 1024:
            raise HTTPException(413, "Too large")
        update = json.loads(body)
        uid = update.get("update_id")
        if not isinstance(uid, int):
            raise HTTPException(400, "Missing update ID")
        with lock:
            if db.execute("SELECT 1 FROM updates WHERE id=?", (uid,)).fetchone():
                return {"ok": True}
            msg = update.get("message")
            callback = update.get("callback_query")
            if (
                msg
                and msg.get("chat", {}).get("type") == "private"
                and msg.get("text", "").startswith("/start launcher_")
            ):
                sid = msg["text"].split("launcher_", 1)[1]
                who = msg["from"]
                row = db.execute(
                    "SELECT * FROM sessions WHERE id=? AND expires>? AND used=0",
                    (sid, time.time()),
                ).fetchone()
                if row and (
                    row["telegram_id"] is None or row["telegram_id"] == who["id"]
                ):
                    send(
                        "sendMessage",
                        {
                            "chat_id": who["id"],
                            "text": "Подтвердить вход в StudHub Launcher? Подтверждайте только вход, который начали вы.",
                            "reply_markup": {
                                "inline_keyboard": [
                                    [
                                        {
                                            "text": "Подтвердить вход",
                                            "callback_data": "launcher:" + sid,
                                        }
                                    ]
                                ]
                            },
                        },
                    )
                    db.execute(
                        "UPDATE sessions SET telegram_id=? WHERE id=?", (who["id"], sid)
                    )
                    db.execute(
                        "INSERT INTO profiles(telegram_id,telegram_username) VALUES(?,?) ON CONFLICT(telegram_id) DO UPDATE SET telegram_username=excluded.telegram_username",
                        (who["id"], who.get("username")),
                    )
            elif callback and callback.get("data", "").startswith("launcher:"):
                sid, tg = callback["data"][9:], callback["from"]["id"]
                changed = db.execute(
                    "UPDATE sessions SET approved=1 WHERE id=? AND telegram_id=? AND expires>? AND used=0 AND approved=0",
                    (sid, tg, time.time()),
                ).rowcount
                send(
                    "answerCallbackQuery",
                    {
                        "callback_query_id": callback["id"],
                        "text": "Вход подтверждён" if changed else "Код недействителен",
                    },
                )
            db.execute("INSERT INTO updates VALUES(?)", (uid,))
        return {"ok": True}

    @app.get("/profile")
    def get_profile(authorization: str = Header(default="")):
        with lock:
            return profile(user(authorization))

    @app.put("/profile/nickname")
    def nickname(body: Nick, authorization: str = Header(default="")):
        with lock:
            tg = user(authorization)
            try:
                db.execute(
                    "UPDATE profiles SET nickname=? WHERE telegram_id=?",
                    (body.nickname, tg),
                )
            except sqlite3.IntegrityError:
                raise HTTPException(409, "Nickname already in use")
            return profile(tg)

    @app.post("/auth/logout")
    def logout(authorization: str = Header(default="")):
        with lock:
            user(authorization)
            db.execute(
                "DELETE FROM tokens WHERE hash=?", (hash_secret(authorization[7:]),)
            )
        return {"ok": True}

    @app.get("/server/status")
    def status():
        address = os.getenv("MINECRAFT_SERVER", "")
        if not address:
            raise HTTPException(503, "Minecraft server not configured")
        from mcstatus import JavaServer

        try:
            s = JavaServer.lookup(address, timeout=5).status()
            return {
                "online": True,
                "players": s.players.online,
                "max_players": s.players.max,
                "version": s.version.name,
            }
        except Exception:
            raise HTTPException(503, "No server connection")

    @app.get("/news")
    def news():
        path = Path(os.getenv("STUDHUB_NEWS", "news.json"))
        if not path.exists():
            return []
        return json.loads(path.read_text("utf8"))

    app.state.db = db
    return app


app = create_app()
