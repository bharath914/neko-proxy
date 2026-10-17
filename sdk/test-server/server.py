#!/usr/bin/env python3
"""Local HTTP server for SDK sample apps, tests and benchmarks. No dependencies.

    python3 sdk/test-server/server.py --port 18080

Routes: /json (1 KB) /json/100k /json/5m /binary /image.png /gzip /redirect /redirect/2
/slow?ms=500 /status/<code> /sse?n=10&ms=100 /stream (chunked) /echo (POST, PUT, ...)
/auth (401 with a challenge, 200 with Authorization) /headers /health
"""
import argparse, gzip, json, os, sys, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs


def filler(n: int) -> bytes:
    item = b'{"id":%d,"name":"Item %d","tags":["a","b","c"],"price":12.5},'
    out, i = [], 0
    total = 2
    while total < n:
        s = item % (i, i)
        out.append(s)
        total += len(s)
        i += 1
    body = b'{"items":[' + b"".join(out).rstrip(b",") + b"]}"
    return body


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    # Without this, headers and body go out as two small writes and Nagle plus delayed ACK adds 40 ms.
    disable_nagle_algorithm = True

    def log_message(self, *a):  # quiet
        if os.environ.get("TEST_SERVER_LOG"):
            super().log_message(*a)

    def _send(self, code, body=b"", ctype="application/json", headers=None):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def _route(self):
        u = urlparse(self.path)
        q = {k: v[0] for k, v in parse_qs(u.query).items()}
        p = u.path
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else b""
        if p == "/health":
            return self._send(200, b"ok", "text/plain")
        if p == "/json":
            return self._send(200, filler(1024))
        if p == "/json/100k":
            return self._send(200, filler(100 * 1024))
        if p == "/json/5m":
            return self._send(200, filler(5 * 1024 * 1024))
        if p == "/binary":
            return self._send(200, os.urandom(200_000), "application/octet-stream")
        if p == "/image.png":
            return self._send(200, b"\x89PNG\r\n\x1a\n" + os.urandom(50_000), "image/png")
        if p == "/gzip":
            data = gzip.compress(filler(20_000))
            return self._send(200, data, "application/json", {"Content-Encoding": "gzip"})
        if p.startswith("/redirect"):
            hops = int(p.rsplit("/", 1)[-1]) if p.rsplit("/", 1)[-1].isdigit() else 1
            target = "/json" if hops <= 1 else f"/redirect/{hops - 1}"
            return self._send(302, b"", "text/plain", {"Location": target})
        if p == "/slow":
            time.sleep(int(q.get("ms", "500")) / 1000)
            return self._send(200, b'{"slow":true}')
        if p.startswith("/status/"):
            code = int(p.rsplit("/", 1)[-1])
            return self._send(code, json.dumps({"status": code}).encode())
        if p == "/sse":
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            for i in range(int(q.get("n", "10"))):
                chunk = f"data: tick {i}\n\n".encode()
                self.wfile.write(b"%x\r\n%s\r\n" % (len(chunk), chunk))
                self.wfile.flush()
                time.sleep(int(q.get("ms", "100")) / 1000)
            self.wfile.write(b"0\r\n\r\n")
            return
        if p == "/stream":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            for i in range(20):
                chunk = filler(2000)
                self.wfile.write(b"%x\r\n%s\r\n" % (len(chunk), chunk))
                self.wfile.flush()
                time.sleep(0.02)
            self.wfile.write(b"0\r\n\r\n")
            return
        if p == "/echo":
            return self._send(200, json.dumps({
                "method": self.command, "path": self.path, "headers": dict(self.headers),
                "body": body.decode("utf-8", "replace"),
            }).encode())
        if p == "/auth":
            if self.headers.get("Authorization"):
                return self._send(200, b'{"authed":true}')
            return self._send(401, b'{"error":"auth required"}', "application/json", {"WWW-Authenticate": 'Basic realm="test"'})
        if p == "/headers":
            return self._send(200, json.dumps(dict(self.headers)).encode())
        return self._send(404, b'{"error":"not found"}')

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = do_OPTIONS = _route


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=18080)
    ap.add_argument("--bind", default="0.0.0.0")
    a = ap.parse_args()
    srv = ThreadingHTTPServer((a.bind, a.port), Handler)
    srv.daemon_threads = True
    print(f"test server on {a.bind}:{srv.server_address[1]}", flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
