#!/usr/bin/env python3
"""Exercise command consent through the MCP wire protocol on isolated stores."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile
import time
import unittest

BINARY = str(Path(sys.argv.pop(1)).resolve())
REQUEST_ID = "1" * 32
THREAD = "consent-test-thread"
COMMAND = "printf consent-test"


class ConsentWireTest(unittest.TestCase):
    def exchange(self, *, answer=None, capabilities=None, session=THREAD,
                 age=0, change=None, arguments=None):
        with tempfile.TemporaryDirectory(prefix="ljos-consent-") as tmp:
            root = Path(tmp)
            store = root / "ljos" / "approvals"
            store.mkdir(parents=True, mode=0o700)
            path = store / "requests.json"
            request = {
                "id": REQUEST_ID, "created": int(time.time()) - age, "approved": False,
                "scope": {
                    "session": THREAD, "shape": "DenyOnly", "tool": "Bash",
                    "cwd": tmp, "command": COMMAND, "pattern": "printf*",
                    "reason": "The test requires explicit consent.",
                },
            }
            path.write_text(json.dumps([request]))
            path.chmod(0o600)
            env = dict(os.environ, XDG_RUNTIME_DIR=tmp, LJOS_SEAT="consent-test")
            with subprocess.Popen([BINARY], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, env=env, text=True) as proc:
                def send(value):
                    proc.stdin.write(json.dumps(value) + "\n")
                    proc.stdin.flush()

                def read():
                    with selectors.DefaultSelector() as selector:
                        selector.register(proc.stdout, selectors.EVENT_READ)
                        self.assertTrue(selector.select(10), "MCP response timed out")
                    line = proc.stdout.readline()
                    self.assertTrue(line, "MCP server closed without an answer")
                    return json.loads(line)

                def response(identifier):
                    while True:
                        value = read()
                        if value.get("id") == identifier:
                            return value
                        self.assertNotIn("id", value, value)

                try:
                    send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                        "protocolVersion": "2025-11-25",
                        "clientInfo": {"name": "consent-wire-test", "version": "1"},
                        "capabilities": capabilities if capabilities is not None
                        else {"elicitation": {"form": {}}},
                    }})
                    self.assertIn("result", response(1))
                    send({"jsonrpc": "2.0", "method": "notifications/initialized"})
                    send({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
                        "name": "ljos_request_approval",
                        "arguments": arguments if arguments is not None else {"id": REQUEST_ID},
                        "_meta": {"thread_id": session},
                    }})
                    while True:
                        value = read()
                        if value.get("method") == "elicitation/create":
                            params = value["params"]
                            self.assertIn(json.dumps(COMMAND), params["message"])
                            self.assertIn(json.dumps(tmp), params["message"])
                            self.assertIn(THREAD, params["message"])
                            self.assertIn(request["scope"]["reason"], params["message"])
                            schema = params["requestedSchema"]
                            self.assertEqual(schema["required"], ["approve"])
                            self.assertIs(schema["properties"]["approve"]["default"], False)
                            self.assertIsNotNone(answer, "unexpected consent request")
                            if change:
                                rows = json.loads(path.read_text())
                                change(rows)
                                path.write_text(json.dumps(rows))
                            send({"jsonrpc": "2.0", "id": value["id"], "result": answer})
                        elif value.get("id") == 2:
                            return value, json.loads(path.read_text())
                        else:
                            self.assertNotIn("id", value, value)
                finally:
                    proc.terminate()
                    try:
                        proc.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait()

    def assert_blocked(self, **kwargs):
        reply, rows = self.exchange(**kwargs)
        self.assertTrue("error" in reply or reply.get("result", {}).get("isError"), reply)
        self.assertFalse(any(row["approved"] for row in rows))

    def test_accept_records_consent(self):
        reply, rows = self.exchange(answer={"action": "accept", "content": {"approve": True}})
        self.assertNotIn("error", reply)
        self.assertFalse(reply["result"].get("isError", False))
        self.assertTrue(rows[0]["approved"])

    def test_empty_legacy_capability_supports_forms(self):
        reply, rows = self.exchange(capabilities={"elicitation": {}},
                                   answer={"action": "accept", "content": {"approve": True}})
        self.assertNotIn("error", reply)
        self.assertTrue(rows[0]["approved"])

    def test_decline_cancel_false_missing_and_string_do_not_grant(self):
        for action in ("decline", "cancel", "accept"):
            for content in (None, {}, {"approve": False}, {"approve": "true"}):
                with self.subTest(action=action, content=content):
                    answer = {"action": action}
                    if content is not None:
                        answer["content"] = content
                    self.assert_blocked(answer=answer)
        for action in ("decline", "cancel"):
            with self.subTest(action=action, true_field=True):
                self.assert_blocked(answer={"action": action, "content": {"approve": True}})

    def test_no_form_capability_leaves_request_blocked(self):
        for caps in ({}, {"elicitation": {"url": {}}}):
            with self.subTest(capabilities=caps):
                self.assert_blocked(capabilities=caps)

    def test_another_conversation_cannot_request_the_grant(self):
        self.assert_blocked(session="different-test-thread")

    def test_tool_arguments_cannot_supply_consent(self):
        self.assert_blocked(arguments={"id": REQUEST_ID, "approve": True})

    def test_expired_request_cannot_open_a_dialog(self):
        self.assert_blocked(age=901)

    def test_expiry_during_confirmation_does_not_grant(self):
        def expire(rows):
            rows[0]["created"] -= 901
        self.assert_blocked(answer={"action": "accept", "content": {"approve": True}},
                            change=expire)

    def test_changed_command_during_confirmation_does_not_grant(self):
        def change(rows):
            rows[0]["scope"]["command"] += " different"
        self.assert_blocked(answer={"action": "accept", "content": {"approve": True}},
                            change=change)


if __name__ == "__main__":
    unittest.main()
