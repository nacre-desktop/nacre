"""Typed NetworkManager reader peers derived from the existing nmcli inputs."""

BASE = "/org/freedesktop/NetworkManager"
DESTINATION = "org.freedesktop.NetworkManager"
PREFIX = ["--terse", "--escape", "yes", "--colors", "no", "--wait", "5"]
DEVICE_ARGS = [*PREFIX, "--fields", "DEVICE,TYPE,STATE", "device", "status"]
AP_ARGS = [
    *PREFIX,
    "--fields",
    "IN-USE,SSID,BSSID,SIGNAL,FREQ,DEVICE",
    "device",
    "wifi",
    "list",
    "--rescan",
    "no",
]


def fields(line):
    values, word, escaped = [], "", False
    for char in line:
        if escaped:
            if char not in (":", "\\"):
                return None
            word += char
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == ":":
            values.append(word)
            word = ""
        else:
            word += char
    return None if escaped else [*values, word]


def rules(case):
    commands = case.get("commands", {}).get("nmcli", [])
    if not commands:
        return []
    result = []

    def effect(argv):
        return {"command": "nmcli", "argv": argv, "stdin": ""}

    def method(path, interface, member, signature, body, reply, effects=()):
        result.append(
            {
                "match": {
                    "destination": DESTINATION,
                    "path": path,
                    "interface": interface,
                    "member": member,
                    "signature": signature,
                    "body": body,
                },
                "reply": reply,
                "effects": list(effects),
            }
        )

    def get(path, interface, key, kind, value, effects=()):
        method(
            path,
            "org.freedesktop.DBus.Properties",
            "Get",
            "ss",
            [interface, key],
            {"signature": "v", "body": [{"variant": kind, "value": value}]},
            effects,
        )

    radio = next(
        (row for row in commands if row.get("argv") == [*PREFIX, "radio", "wifi"]),
        commands[0],
    )
    if radio.get("code", 0):
        method(
            BASE,
            "org.freedesktop.DBus.Properties",
            "Get",
            "ss",
            [DESTINATION, "WirelessEnabled"],
            {
                "error": DESTINATION + ".Unavailable",
                "text": "Fixture NetworkManager unavailable",
            },
            [effect([*PREFIX, "radio", "wifi"])],
        )
        return result
    text = radio.get("stdout", "").strip()
    get(
        BASE,
        DESTINATION,
        "WirelessEnabled",
        "b" if text in ("enabled", "disabled") else "s",
        text == "enabled" if text in ("enabled", "disabled") else text,
        [effect([*PREFIX, "radio", "wifi"])],
    )
    devices = next((row for row in commands if row.get("argv") == DEVICE_ARGS), {})
    rows = [
        row
        for line in devices.get("stdout", "").splitlines()
        if (row := fields(line)) and len(row) == 3
    ]
    paths = [BASE + "/Devices/" + str(index + 1) for index in range(len(rows))]
    method(
        BASE,
        DESTINATION,
        "GetDevices",
        "",
        [],
        {"signature": "ao", "body": [paths]},
        [effect(DEVICE_ARGS)],
    )
    aps = next((row for row in commands if "argv" not in row), {})
    wifi = [
        (path, row) for path, row in zip(paths, rows, strict=True) if row[1] == "wifi"
    ]
    for path, row in zip(paths, rows, strict=True):
        interface, kind, state = row
        get(
            path, DESTINATION + ".Device", "DeviceType", "u", 2 if kind == "wifi" else 1
        )
        get(path, DESTINATION + ".Device", "Interface", "s", interface)
        get(
            path,
            DESTINATION + ".Device",
            "State",
            "u",
            100 if state == "connected" else 30,
        )
    for index, (path, row) in enumerate(wifi):
        ap_rows = []
        for line in aps.get("stdout", "").splitlines():
            ap = fields(line)
            if ap and len(ap) == 6 and ap[5] == row[0]:
                ap_rows.append(ap)
            elif line and (not ap or len(ap) != 6):
                # The frozen text transport rejected this row. Its typed peer is
                # malformed SSID data, not an invented valid escaped network.
                ap_rows.append(
                    ["", "\0invalid transport row", "invalid", "0", "0", row[0]]
                )
        ap_paths = [
            BASE + f"/AccessPoint/{index + 1}_{i + 1}" for i in range(len(ap_rows))
        ]
        get(
            path,
            DESTINATION + ".Device.Wireless",
            "AccessPoints",
            "ao",
            ap_paths,
            [effect(AP_ARGS)] if index == 0 else [],
        )
        active = next(
            (p for p, ap in zip(ap_paths, ap_rows, strict=True) if ap[0] == "*"), "/"
        )
        get(path, DESTINATION + ".Device.Wireless", "ActiveAccessPoint", "o", active)
        for ap_path, ap in zip(ap_paths, ap_rows, strict=True):
            get(
                ap_path,
                DESTINATION + ".AccessPoint",
                "Ssid",
                "ay",
                list(ap[1].encode()),
            )
            get(ap_path, DESTINATION + ".AccessPoint", "HwAddress", "s", ap[2])
            get(ap_path, DESTINATION + ".AccessPoint", "Strength", "y", int(ap[3]))
            get(
                ap_path,
                DESTINATION + ".AccessPoint",
                "Frequency",
                "u",
                int(ap[4].split()[0]),
            )
    return result
