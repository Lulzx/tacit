#!/usr/bin/env python3
"""Serve the Tacit web app.  /console is the store shell; / is the graph bench."""

from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os

HERE = os.path.dirname(os.path.abspath(__file__))
os.chdir(HERE)


class Handler(SimpleHTTPRequestHandler):
    def translate_path(self, path):
        raw = path.split("?", 1)[0]
        if raw in ("/console", "/console/"):
            path = "/index.html"
        return SimpleHTTPRequestHandler.translate_path(self, path)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, fmt, *args):
        print("%s - %s" % (self.log_date_time_string(), fmt % args))


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "8765"))
    print("Tacit web: http://127.0.0.1:%d/" % port)
    print("console:   http://127.0.0.1:%d/console" % port)
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
