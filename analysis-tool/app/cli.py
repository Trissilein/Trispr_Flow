from __future__ import annotations

import argparse
import json
import socket
import sys
import threading
import time
from pathlib import Path
from typing import Any

import requests
import uvicorn
import webview

from .web_server import STATE, create_app

EXIT_SUCCESS = 0
EXIT_RUNTIME_MISSING = 10
EXIT_MODEL_LOAD_FAILURE = 11
EXIT_INVALID_AUDIO = 12
EXIT_STARTUP_FAILURE = 20


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _find_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def _wait_for_server(base_url: str, server_thread: threading.Thread, timeout_s: float = 15.0) -> None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if not server_thread.is_alive():
            raise RuntimeError("Analysis web server exited before becoming ready.")
        try:
            response = requests.get(f"{base_url}/api/ping", timeout=0.4)
            if response.ok:
                return
        except Exception:
            pass
        time.sleep(0.1)
    raise RuntimeError("Timed out while waiting for analysis web server startup.")


def _start_server(audio_path: str | None, source: str | None) -> tuple[uvicorn.Server, threading.Thread, str]:
    port = _find_free_port()
    app = create_app()
    STATE.set_startup(audio_path, source)

    config = uvicorn.Config(
        app,
        host="127.0.0.1",
        port=port,
        log_level="warning",
        access_log=False,
    )
    server = uvicorn.Server(config)
    thread = threading.Thread(target=server.run, daemon=True)
    thread.start()

    base_url = f"http://127.0.0.1:{port}"
    _wait_for_server(base_url, thread)
    return server, thread, base_url


def _print_payload(payload: dict[str, Any], as_json: bool) -> None:
    if as_json:
        print(json.dumps(payload, ensure_ascii=True))
        return

    for key, value in payload.items():
        print(f"{key}: {value}")


def command_status(args: argparse.Namespace) -> int:
    payload = {
        "installed": True,
        "mode": "python",
        "entrypoint": str((_repo_root() / "analysis-tool" / "main.py").resolve()),
        "python": sys.executable,
    }
    _print_payload(payload, args.json)
    return EXIT_SUCCESS


def command_doctor(args: argparse.Namespace) -> int:
    worker_script = (_repo_root() / "sidecar" / "vibevoice-asr" / "worker_once.py").resolve()
    static_index = (Path(__file__).resolve().parent / "static" / "index.html").resolve()

    checks = {
        "worker_script_exists": worker_script.exists(),
        "worker_script": str(worker_script),
        "static_index_exists": static_index.exists(),
        "static_index": str(static_index),
        "python": sys.executable,
    }

    code = EXIT_SUCCESS
    if not checks["static_index_exists"]:
        code = EXIT_STARTUP_FAILURE
    elif not checks["worker_script_exists"]:
        code = EXIT_RUNTIME_MISSING

    payload = {
        "ok": code == EXIT_SUCCESS,
        "checks": checks,
        "exit_code": code,
    }
    _print_payload(payload, args.json)
    return code


def command_open(args: argparse.Namespace) -> int:
    audio = args.audio
    if audio:
        input_path = Path(audio)
        if not input_path.exists():
            print(f"Invalid audio input: {input_path}", file=sys.stderr)
            return EXIT_INVALID_AUDIO
        audio = str(input_path.resolve())

    try:
        server, thread, base_url = _start_server(audio_path=audio, source=args.source)
    except Exception as exc:
        print(f"Failed to initialize analysis server: {exc}", file=sys.stderr)
        return EXIT_STARTUP_FAILURE

    try:
        webview.create_window(
            "Trispr Analysis",
            base_url,
            width=1400,
            height=920,
            min_size=(1100, 700),
        )
        webview.start(debug=bool(args.debug))
    except Exception as exc:
        print(f"Failed to start analysis window: {exc}", file=sys.stderr)
        server.should_exit = True
        thread.join(timeout=2.0)
        return EXIT_STARTUP_FAILURE

    server.should_exit = True
    thread.join(timeout=2.0)
    return EXIT_SUCCESS


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="trispr-analysis", description="Trispr Analysis CLI")
    subparsers = parser.add_subparsers(dest="command", required=True)

    open_parser = subparsers.add_parser("open", help="Open the analysis app window")
    open_parser.add_argument("--audio", type=str, default=None, help="Absolute audio file path")
    open_parser.add_argument("--source", type=str, default="trispr-flow", help="Launch source")
    open_parser.add_argument("--debug", action="store_true", help="Enable debug mode for webview")
    open_parser.set_defaults(handler=command_open)

    status_parser = subparsers.add_parser("status", help="Print runtime status")
    status_parser.add_argument("--json", action="store_true", help="Emit JSON output")
    status_parser.set_defaults(handler=command_status)

    doctor_parser = subparsers.add_parser("doctor", help="Run local runtime diagnostics")
    doctor_parser.add_argument("--json", action="store_true", help="Emit JSON output")
    doctor_parser.set_defaults(handler=command_doctor)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    handler = getattr(args, "handler", None)
    if handler is None:
        parser.print_help()
        return EXIT_STARTUP_FAILURE
    return int(handler(args))
