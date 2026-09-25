#!/usr/bin/env python3
"""bundle-mirror.py — a read-through, range-aware cache of the TeX bundle host.

The stills harness's cold compile (State 4) starts from an empty engine cache
on purpose, so every run needs the bundle's files over HTTP. The engine reads
the bundle with plain GETs (the index) and ranged GETs (each file); this
server answers each distinct (method, path, Range) once from upstream and
replays the stored response ever after, so a cold compile stays cold for the
app but local for the network.

Bytes are passed through untouched: the app's digest pin still judges them.
Debug builds point the engine here with MALEFICIUM_DEV_BUNDLE_URL.

Usage: bundle-mirror.py <port> <cache-dir> [--offline]
  --offline (or MIRROR_OFFLINE=1): never contact upstream; a miss is a 502,
  which proves a run needed no network.
Prints "mirror: listening on <port>" once ready; one line per request to stderr.
"""

import hashlib
import json
import os
import sys
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

UPSTREAM = "https://data1b.fullyjustified.net"
PASS_HEADERS = ("Content-Type", "Content-Range", "Accept-Ranges", "Last-Modified", "ETag")

PORT = int(sys.argv[1])
CACHE = sys.argv[2]
OFFLINE = "--offline" in sys.argv[3:] or os.environ.get("MIRROR_OFFLINE") == "1"
os.makedirs(CACHE, exist_ok=True)


def key(method, path, rng):
    return hashlib.sha256(f"{method}\n{path}\n{rng}".encode()).hexdigest()


def fetch(method, path, rng, agent):
    """(status, headers, body) from upstream; HTTP errors are answers too.
    The client's User-Agent goes along: the host refuses urllib's (403)."""
    req = urllib.request.Request(UPSTREAM + path, method=method)
    req.add_header("User-Agent", agent or "maleficium-bundle-mirror")
    if rng:
        req.add_header("Range", rng)
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, {h: r.headers[h] for h in PASS_HEADERS if r.headers[h]}, r.read()
    except urllib.error.HTTPError as e:
        return e.code, {h: e.headers[h] for h in PASS_HEADERS if e.headers[h]}, e.read()


class Mirror(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def serve(self, method):
        rng = self.headers.get("Range", "")
        k = key(method, self.path, rng)
        meta_p, body_p = os.path.join(CACHE, k + ".json"), os.path.join(CACHE, k + ".body")
        if os.path.exists(meta_p) and os.path.exists(body_p):
            with open(meta_p) as f:
                status, headers = json.load(f)
            with open(body_p, "rb") as f:
                body = f.read()
            how = "hit"
        elif OFFLINE:
            status, headers, body, how = 502, {}, b"mirror offline: not cached\n", "miss-offline"
        else:
            try:
                status, headers, body = fetch(method, self.path, rng, self.headers.get("User-Agent"))
            except (urllib.error.URLError, OSError) as e:
                status, headers, body, how = 502, {}, f"mirror upstream: {e}\n".encode(), "upstream-error"
            else:
                how = "fetched"
                # Only real content is kept: a refusal or an outage must be
                # asked again next time, never replayed.
                if status in (200, 206):
                    # Body first, meta last: a crash mid-write never leaves a
                    # meta that promises a body it does not have.
                    tmp = body_p + ".tmp"
                    with open(tmp, "wb") as f:
                        f.write(body)
                    os.replace(tmp, body_p)
                    with open(meta_p + ".tmp", "w") as f:
                        json.dump([status, headers], f)
                    os.replace(meta_p + ".tmp", meta_p)
        self.send_response(status)
        for h, v in headers.items():
            self.send_header(h, v)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if method == "GET":
            self.wfile.write(body)
        sys.stderr.write(f"mirror: {how} {status} {method} {self.path} {rng}\n")

    def do_GET(self):
        self.serve("GET")

    def do_HEAD(self):
        self.serve("HEAD")

    def log_message(self, *args):
        pass


server = ThreadingHTTPServer(("127.0.0.1", PORT), Mirror)
print(f"mirror: listening on {PORT}{' (offline)' if OFFLINE else ''}", flush=True)
server.serve_forever()
