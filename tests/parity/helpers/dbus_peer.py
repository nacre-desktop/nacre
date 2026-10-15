#!/usr/bin/env python3
"""Strict private D-Bus fixture peer with explicit process/action equivalences."""

import asyncio
import fcntl
import json
from pathlib import Path
import sys

from dbus_next import Message, MessageType, Variant
from dbus_next.aio import MessageBus


def plain(value):
    if isinstance(value, bytes):
        return list(value)
    if isinstance(value, Variant):
        return {"variant": value.signature, "value": plain(value.value)}
    if isinstance(value, list):
        return [plain(item) for item in value]
    if isinstance(value, dict):
        return {key: plain(item) for key, item in value.items()}
    return value


def typed(value):
    if isinstance(value, dict):
        if set(value) == {"variant", "value"}:
            return Variant(
                value["variant"],
                bytes(value["value"])
                if value["variant"] == "ay"
                else typed(value["value"]),
            )
        return {key: typed(item) for key, item in value.items()}
    if isinstance(value, list):
        return [typed(item) for item in value]
    return value


def append(path, value):
    with path.open("a") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        stream.write(json.dumps(value) + "\n")


async def main():
    root = Path(sys.argv[1])
    rules = json.loads((root / "dbus-fixtures.json").read_text())
    address = "unix:path=" + str(root / "runtime/parity-bus")
    bus = await MessageBus(bus_address=address).connect()
    for name in sorted({rule["match"]["destination"] for rule in rules}):
        await bus.request_name(name)

    def handle(message):
        if message.message_type != MessageType.METHOD_CALL:
            return None
        call = {
            field: getattr(message, field)
            for field in ("destination", "path", "interface", "member", "signature")
        }
        call["body"] = plain(message.body)
        append(root / "dbus-wire.jsonl", call)
        rule = next((row for row in rules if row["match"] == call), None)
        if rule is None:
            append(root / "effects.jsonl", {"unexpected_dbus": call})
            return Message.new_error(
                message, "org.nacre.Parity.Unscripted", "Unscripted call"
            )
        for effect in rule.get("effects", []):
            append(root / "effects.jsonl", effect)
        reply = rule.get("reply", {})
        if "error" in reply:
            return Message.new_error(message, reply["error"], reply.get("text", ""))
        return Message.new_method_return(
            message, reply.get("signature", ""), typed(reply.get("body", []))
        )

    bus.add_message_handler(handle)
    print('{"ready":true}', flush=True)
    await bus.wait_for_disconnect()


if __name__ == "__main__":
    asyncio.run(main())
