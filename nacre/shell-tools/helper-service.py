#!/usr/bin/env python3
"""Shell-owned, bounded JSON-lines RPC for explicitly supported helper operations."""

from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading

HERE = Path(__file__).resolve().parent
MAX_REQUEST = 4 * 1024 * 1024
MAX_RESPONSE = 8 * 1024 * 1024
MAX_PENDING = 64
TOOLS = {
    "menu-backend",
    "dashboard-data",
    "dashboard-media",
    "launcher-preferences",
    "notification-history",
    "network-state",
}


class Helpers:
    def __init__(self):
        self.modules = {}
        self.locks = {name: threading.Lock() for name in TOOLS}

    def module(self, name):
        if name not in self.modules:
            directory = (
                Path(os.environ.get("NACRE_DASHBOARD_TOOLS", str(HERE)))
                if name in ("dashboard-data", "dashboard-media")
                else HERE
            )
            spec = importlib.util.spec_from_file_location(
                "nacre_helper_" + name.replace("-", "_"), directory / (name + ".py")
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            self.modules[name] = module
        return self.modules[name]

    def execute(self, request):
        name, arguments, payload = (
            request["tool"],
            request["arguments"],
            request["input"],
        )
        # Preserve each helper's serialization, while independent helpers can overlap.
        with self.locks[name]:
            module = self.module(name)
            result = self.dispatch(module, name, arguments, payload)
        return "" if result is None else json.dumps(result, allow_nan=False) + "\n"

    @staticmethod
    def dispatch(module, name, arguments, payload):
        if name == "network-state" and not arguments and payload is None:
            return module.snapshot()
        if name == "dashboard-media" and len(arguments) == 1 and payload is None:
            return module.track_data(json.loads(arguments[0]))
        if name == "dashboard-data" and 1 <= len(arguments) <= 2 and payload is None:
            return module.execute(
                arguments[0], arguments[1] if len(arguments) == 2 else ""
            )
        if name == "launcher-preferences" and len(arguments) == 1:
            action = arguments[0]
            if action == "load" and payload is None:
                return module.initialize()
            if action in ("favorite", "hide", "record") and isinstance(payload, dict):
                return module.change(action, payload)
        if name == "notification-history" and len(arguments) == 1:
            if arguments[0] == "load" and payload is None:
                return module.load()
            if arguments[0] == "save" and isinstance(payload, list):
                module.save(payload)
                return None
        if name == "menu-backend" and payload is None:
            if arguments == ["state"] or arguments == ["state", "--force"]:
                return module.snapshot(arguments[-1] == "--force")
            readers = {
                "files": module.search_files,
                "convert": module.convert_units,
                "packages": module.search_packages,
                "ownership": module.ownership,
            }
            if len(arguments) == 2 and arguments[0] in readers:
                return readers[arguments[0]](arguments[1])
        raise ValueError("Unsupported helper operation")


def validate(request):
    if not isinstance(request, dict) or set(request) != {
        "id",
        "tool",
        "arguments",
        "input",
    }:
        raise ValueError("Invalid helper request")
    if type(request["id"]) is not int or not 0 < request["id"] <= 9007199254740991:
        raise ValueError("Invalid helper request ID")
    if not isinstance(request["tool"], str) or request["tool"] not in TOOLS:
        raise ValueError("Unsupported helper")
    if (
        not isinstance(request["arguments"], list)
        or len(request["arguments"]) > 8
        or any(
            not isinstance(value, str) or len(value) > 65536 or "\0" in value
            for value in request["arguments"]
        )
    ):
        raise ValueError("Invalid helper arguments")
    return request


def invalid_constant(value):
    raise ValueError("Non-finite JSON value")


def serve(input_stream=None, output_stream=None, helpers=None):
    input_stream = input_stream if input_stream is not None else sys.stdin.buffer
    output_stream = output_stream if output_stream is not None else sys.stdout
    helpers = helpers or Helpers()
    output_lock = threading.Lock()
    active_lock = threading.Lock()
    active = set()
    slots = threading.BoundedSemaphore(MAX_PENDING)

    def send(value):
        packet = json.dumps(value, allow_nan=False)
        if len(packet) > MAX_RESPONSE:
            packet = json.dumps(
                {
                    "id": value.get("id", 0),
                    "code": 1,
                    "output": json.dumps({"error": "Helper response is too large"}),
                }
            )
        with output_lock:
            print(packet, file=output_stream, flush=True)

    def execute(request):
        try:
            response = {
                "id": request["id"],
                "code": 0,
                "output": helpers.execute(request),
            }
        except Exception as error:  # noqa: BLE001 - isolate every request at the worker boundary
            response = {
                "id": request["id"],
                "code": 1,
                "output": json.dumps(
                    {"error": str(error)[:240] or "Helper action failed"}
                ),
            }
        try:
            send(response)
        finally:
            with active_lock:
                active.discard(request["id"])
            slots.release()

    send({"ready": True, "version": 1})
    # One serial lane per supported helper prevents lock-waiting requests for a
    # busy tool from occupying the workers needed by independent helpers. Pools
    # start threads lazily; the shared pending semaphore still bounds all lanes.
    with ExitStack() as lifetime:
        pools = {
            name: lifetime.enter_context(
                ThreadPoolExecutor(
                    max_workers=1, thread_name_prefix="nacre-helper-" + name
                )
            )
            for name in sorted(TOOLS)
        }
        while line := input_stream.readline(MAX_REQUEST + 1):
            if len(line) > MAX_REQUEST:
                send({"fatal": "Helper request is too large"})
                return 1
            ident = 0
            try:
                request = json.loads(line, parse_constant=invalid_constant)
                if isinstance(request, dict) and type(request.get("id")) is int:
                    ident = request["id"]
                request = validate(request)
                with active_lock:
                    if request["id"] in active:
                        send({"fatal": "Duplicate helper request ID"})
                        return 1
                    if not slots.acquire(blocking=False):
                        send(
                            {
                                "id": ident,
                                "code": 1,
                                "output": json.dumps({"error": "Helper queue is full"}),
                            }
                        )
                        continue
                    active.add(request["id"])
                pools[request["tool"]].submit(execute, request)
            except (ValueError, TypeError, UnicodeError, RecursionError) as error:
                send(
                    {
                        "id": ident,
                        "code": 1,
                        "output": json.dumps({"error": str(error)[:240]}),
                    }
                )
    return 0


if __name__ == "__main__":
    raise SystemExit(serve())
