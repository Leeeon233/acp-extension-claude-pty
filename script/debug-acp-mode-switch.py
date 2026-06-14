#!/usr/bin/env python3
"""Run a real ACP mode-switch session and capture debug artifacts."""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path
from typing import Any


ANSI_RE = re.compile(
    r"(?:\x1b\][^\x07]*(?:\x07|\x1b\\))|(?:\x1b[@-Z\\-_])|(?:\x1b\[[0-?]*[ -/]*[@-~])"
)


class AcpClient:
    def __init__(self, command: list[str], env: dict[str, str], out_dir: Path) -> None:
        self.command = command
        self.env = env
        self.out_dir = out_dir
        self.proc: asyncio.subprocess.Process | None = None
        self.next_id = 1
        self.pending: dict[int, asyncio.Future[dict[str, Any]]] = {}
        self.messages_path = out_dir / "acp-messages.jsonl"
        self.stderr_path = out_dir / "agent-stderr.log"
        self.messages_file = self.messages_path.open("w", encoding="utf-8")
        self.stderr_file = self.stderr_path.open("w", encoding="utf-8")
        self.reader_task: asyncio.Task[None] | None = None
        self.stderr_task: asyncio.Task[None] | None = None

    async def start(self) -> None:
        self.proc = await asyncio.create_subprocess_exec(
            *self.command,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=self.env,
        )
        self.reader_task = asyncio.create_task(self._read_stdout())
        self.stderr_task = asyncio.create_task(self._read_stderr())

    async def stop(self) -> None:
        if self.proc is not None and self.proc.returncode is None:
            self.proc.terminate()
            try:
                await asyncio.wait_for(self.proc.wait(), timeout=5)
            except asyncio.TimeoutError:
                self.proc.kill()
                await self.proc.wait()
        for task in [self.reader_task, self.stderr_task]:
            if task is not None:
                task.cancel()
        self.messages_file.close()
        self.stderr_file.close()

    async def request(
        self, method: str, params: dict[str, Any], timeout: float
    ) -> dict[str, Any]:
        if self.proc is None or self.proc.stdin is None:
            raise RuntimeError("ACP process is not running")
        request_id = self.next_id
        self.next_id += 1
        loop = asyncio.get_running_loop()
        future: asyncio.Future[dict[str, Any]] = loop.create_future()
        self.pending[request_id] = future
        message = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        }
        self._record("client", message)
        self.proc.stdin.write((json.dumps(message, separators=(",", ":")) + "\n").encode())
        await self.proc.stdin.drain()
        response = await asyncio.wait_for(future, timeout=timeout)
        if "error" in response:
            raise RuntimeError(f"{method} failed: {json.dumps(response['error'], ensure_ascii=False)}")
        return response.get("result", {})

    async def _read_stdout(self) -> None:
        assert self.proc is not None and self.proc.stdout is not None
        while True:
            line = await self.proc.stdout.readline()
            if not line:
                break
            text = line.decode("utf-8", errors="replace").rstrip("\n")
            try:
                message = json.loads(text)
            except json.JSONDecodeError:
                self._record("agent-raw", {"line": text})
                continue
            self._record("agent", message)
            if "id" in message and ("result" in message or "error" in message):
                future = self.pending.pop(int(message["id"]), None)
                if future is not None and not future.done():
                    future.set_result(message)
            elif "id" in message and message.get("method") == "session/request_permission":
                await self._answer_permission_request(message)

    async def _read_stderr(self) -> None:
        assert self.proc is not None and self.proc.stderr is not None
        while True:
            line = await self.proc.stderr.readline()
            if not line:
                break
            text = line.decode("utf-8", errors="replace")
            self.stderr_file.write(text)
            self.stderr_file.flush()

    async def _answer_permission_request(self, message: dict[str, Any]) -> None:
        assert self.proc is not None and self.proc.stdin is not None
        params = message.get("params") or {}
        options = params.get("options") or []
        option = choose_permission_option(options)
        response = {
            "jsonrpc": "2.0",
            "id": message["id"],
            "result": {
                "outcome": {
                    "outcome": "selected",
                    "optionId": option,
                }
            },
        }
        self._record("client", response)
        self.proc.stdin.write((json.dumps(response, separators=(",", ":")) + "\n").encode())
        await self.proc.stdin.drain()

    def _record(self, side: str, message: dict[str, Any]) -> None:
        record = {"ts": time.time(), "side": side, "message": message}
        self.messages_file.write(json.dumps(record, ensure_ascii=False) + "\n")
        self.messages_file.flush()


def choose_permission_option(options: list[dict[str, Any]]) -> str:
    for preferred in ("reject_once", "reject", "deny"):
        for option in options:
            if option.get("optionId") == preferred:
                return preferred
    if options:
        return str(options[-1].get("optionId"))
    return "reject_once"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--agent",
        default="target/debug/acp-extension-claude-pty",
        help="ACP adapter binary to run",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=None,
        help="Directory for debug artifacts",
    )
    parser.add_argument(
        "--workdir",
        type=Path,
        default=None,
        help="Disposable project directory to use as ACP cwd",
    )
    parser.add_argument("--startup-timeout", type=int, default=20)
    parser.add_argument("--request-timeout", type=int, default=180)
    parser.add_argument("--first-mode", default="plan")
    parser.add_argument("--second-mode", default="acceptEdits")
    parser.add_argument(
        "--setting-sources",
        default="project",
        help="Value forwarded through CLAUDE_CODE_ACP_SETTING_SOURCES",
    )
    return parser.parse_args()


async def run() -> int:
    args = parse_args()
    repo = Path(__file__).resolve().parents[1]
    agent = Path(args.agent)
    if not agent.is_absolute():
        agent = repo / agent
    if not agent.exists():
        raise SystemExit(f"agent binary not found: {agent}; run `cargo build` first")

    stamp = time.strftime("%Y%m%d-%H%M%S")
    out_dir = args.out_dir or (repo / "target" / "acp-mode-switch-debug" / stamp)
    out_dir.mkdir(parents=True, exist_ok=True)
    pty_dir = out_dir / "pty"
    pty_dir.mkdir(parents=True, exist_ok=True)

    workdir = args.workdir
    temp_workdir: tempfile.TemporaryDirectory[str] | None = None
    if workdir is None:
        temp_workdir = tempfile.TemporaryDirectory(prefix="acp-mode-switch-")
        workdir = Path(temp_workdir.name)
    workdir = workdir.resolve()
    workdir.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q"], cwd=workdir, check=False)

    session_id = str(uuid.uuid4())
    env = os.environ.copy()
    env["CLAUDE_CODE_ACP_DEBUG_PTY_DIR"] = str(pty_dir)
    env["CLAUDE_CODE_ACP_SETTING_SOURCES"] = args.setting_sources
    env.setdefault("RUST_LOG", "warn")

    command = [str(agent), "acp", "--startup-timeout", str(args.startup_timeout)]
    client = AcpClient(command, env, out_dir)
    summary: dict[str, Any] = {
        "sessionId": session_id,
        "workdir": str(workdir),
        "outDir": str(out_dir),
        "agent": str(agent),
        "command": command,
        "firstMode": args.first_mode,
        "secondMode": args.second_mode,
    }
    (out_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    try:
        await client.start()
        await client.request(
            "initialize",
            {
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": {"name": "debug-acp-mode-switch", "version": "0.1.0"},
            },
            timeout=args.request_timeout,
        )
        created = await client.request(
            "session/new",
            {
                "cwd": str(workdir),
                "mcpServers": [],
                "_meta": {"claudeCode": {"sessionId": session_id}},
            },
            timeout=args.request_timeout,
        )
        session_id = created["sessionId"]
        summary["sessionId"] = session_id

        await client.request(
            "session/set_mode",
            {"sessionId": session_id, "modeId": args.first_mode},
            timeout=args.request_timeout,
        )
        await client.request(
            "session/prompt",
            {
                "sessionId": session_id,
                "prompt": [
                    {
                        "type": "text",
                        "text": "Reply exactly ACP_MODE_SWITCH_PLAN_READY. Do not use tools.",
                    }
                ],
            },
            timeout=args.request_timeout,
        )

        await client.request(
            "session/set_mode",
            {"sessionId": session_id, "modeId": args.second_mode},
            timeout=args.request_timeout,
        )
        await client.request(
            "session/prompt",
            {
                "sessionId": session_id,
                "prompt": [
                    {
                        "type": "text",
                        "text": "Reply exactly ACP_MODE_SWITCH_SECOND_READY. Do not use tools.",
                    }
                ],
            },
            timeout=args.request_timeout,
        )
        await client.request(
            "session/close",
            {"sessionId": session_id},
            timeout=args.request_timeout,
        )
        summary["status"] = "ok"
        return_code = 0
    except Exception as exc:
        summary["status"] = "error"
        summary["error"] = str(exc)
        return_code = 1
    finally:
        await client.stop()
        write_pty_text_views(pty_dir)
        if temp_workdir is not None:
            summary["temporaryWorkdirRemoved"] = True
            temp_workdir.cleanup()
        summary["ptyFiles"] = sorted(path.name for path in pty_dir.glob("*"))
        (out_dir / "summary.json").write_text(
            json.dumps(summary, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        print(json.dumps(summary, indent=2, ensure_ascii=False))

    return return_code


def write_pty_text_views(pty_dir: Path) -> None:
    combined: list[str] = []
    for path in sorted(pty_dir.glob("*.ansi")):
        raw = path.read_bytes()
        decoded = raw.decode("utf-8", errors="replace")
        visible = ANSI_RE.sub("", decoded)
        text_path = path.with_suffix(".txt")
        text_path.write_text(visible, encoding="utf-8")
        combined.append(f"===== {path.name} =====\n{visible}\n")
    if combined:
        (pty_dir / "pty-visible-combined.txt").write_text(
            "\n".join(combined), encoding="utf-8"
        )


if __name__ == "__main__":
    if shutil.which("git") is None:
        print("warning: git not found; workspace trust behavior may differ", file=sys.stderr)
    raise SystemExit(asyncio.run(run()))
