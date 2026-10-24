import http.server
import os
import socketserver
import sys


def main():
    # Step 1: port from the command line (default 8000), serve the whole project folder
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.chdir(root)

    # Step 2: a handler that stops the browser caching, because the log files change every second
    class Handler(http.server.SimpleHTTPRequestHandler):
        def end_headers(self):
            self.send_header("Cache-Control", "no-store")
            super().end_headers()

        def log_message(self, *args):
            pass  # keep the terminal quiet

    # Step 3: serve until Ctrl+C
    socketserver.ThreadingTCPServer.allow_reuse_address = True
    with socketserver.ThreadingTCPServer(("0.0.0.0", port), Handler) as server:
        print("Dashboard server running on port", port, "- press Ctrl+C to stop")
        server.serve_forever()


main()