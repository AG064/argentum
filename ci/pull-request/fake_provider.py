#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""A local OpenAI-compatible server that answers every chat request with fixed text.

It is for testing the pull request action without a provider account. It listens
on 127.0.0.1 only and logs each request body to --log so a test can check what
the action sent. With --edit, it first reads one file, then replaces text in it
with edit_text, then answers, so a test can check the fix path.
"""

import argparse
import hashlib
import json
import pathlib
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ANSWER = (
    "## Review\n\n"
    "Fixture review from the local test provider. No model was called.\n\n"
    "- `README.md`: the change reads well.\n"
)


def tool_call(call_id, name, arguments):
    return {"index": 0, "id": call_id, "type": "function",
            "function": {"name": name, "arguments": json.dumps(arguments)}}


def scripted_step(body, edit):
    """The tool call for this request, from how many tool results it already holds."""
    if not edit:
        return None
    done = sum(1 for message in body.get("messages", []) if message.get("role") == "tool")
    path, local, old, new = edit
    if done == 0:
        return tool_call("read-1", "read_text", {"path": path})
    if done == 1:
        digest = hashlib.sha256(pathlib.Path(local).read_bytes()).hexdigest()
        return tool_call("edit-1", "edit_text", {"path": path, "expected_sha256": digest,
                                                 "old_text": old, "new_text": new})
    return None


def handler(answer, log_path, lock, edit=None):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def _json(self, status, body):
            data = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):  # noqa: N802
            if self.path.rstrip("/").endswith("/models"):
                self._json(200, {"object": "list", "data": [{"id": "fixture", "object": "model"}]})
            else:
                self._json(404, {"error": {"message": "not found"}})

        def do_POST(self):  # noqa: N802
            length = int(self.headers.get("Content-Length", "0"))
            body = json.loads(self.rfile.read(length) or b"{}")
            if log_path:
                with lock, open(log_path, "a", encoding="utf-8") as log:
                    log.write(json.dumps({"path": self.path, "body": body}) + "\n")
            if not self.path.rstrip("/").endswith("/chat/completions"):
                self._json(404, {"error": {"message": "not found"}})
                return
            usage = {"prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120}
            step = scripted_step(body, edit)
            if step and body.get("stream"):
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                for chunk in ({"choices": [{"index": 0, "delta": {"role": "assistant",
                                                                    "tool_calls": [step]}}]},
                              {"choices": [{"index": 0, "delta": {},
                                            "finish_reason": "tool_calls"}]},
                              {"choices": [], "usage": usage}):
                    chunk.update({"id": "fixture", "object": "chat.completion.chunk",
                                  "model": body.get("model")})
                    self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
                self.wfile.write(b"data: [DONE]\n\n")
                self.wfile.flush()
                return
            if step:
                self._json(200, {
                    "id": "fixture", "object": "chat.completion", "model": body.get("model"),
                    "choices": [{"index": 0, "finish_reason": "tool_calls",
                                 "message": {"role": "assistant", "content": None,
                                             "tool_calls": [step]}}],
                    "usage": usage,
                })
                return
            if not body.get("stream"):
                self._json(200, {
                    "id": "fixture", "object": "chat.completion", "model": body.get("model"),
                    "choices": [{"index": 0, "finish_reason": "stop",
                                 "message": {"role": "assistant", "content": answer}}],
                    "usage": usage,
                })
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            chunks = [
                {"choices": [{"index": 0, "delta": {"role": "assistant", "content": answer}}]},
                {"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]},
                {"choices": [], "usage": usage},
            ]
            for chunk in chunks:
                chunk.update({"id": "fixture", "object": "chat.completion.chunk",
                              "model": body.get("model")})
                self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--port-file", type=pathlib.Path, required=True)
    parser.add_argument("--log", type=pathlib.Path)
    parser.add_argument("--answer-file", type=pathlib.Path)
    parser.add_argument("--edit", nargs=4, metavar=("PATH", "LOCAL_FILE", "OLD", "NEW"),
                        help="read PATH, then replace OLD with NEW in it; LOCAL_FILE is the "
                             "same file on disk, for its SHA-256")
    arguments = parser.parse_args()
    answer = arguments.answer_file.read_text(encoding="utf-8") if arguments.answer_file else ANSWER
    server = ThreadingHTTPServer(("127.0.0.1", arguments.port),
                                 handler(answer, arguments.log, threading.Lock(),
                                         arguments.edit))
    arguments.port_file.write_text(str(server.server_address[1]), encoding="ascii")
    server.serve_forever()


if __name__ == "__main__":
    main()
