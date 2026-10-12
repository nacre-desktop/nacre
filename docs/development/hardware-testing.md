# Hardware acceptance checklist

Run this checklist on a disposable VM first, then tester machines. Use the default
profile and a reviewed, clean task revision. Preserve the tester's configuration
and rollback; do not change private profiles, grant AI permissions, install vendor
monitoring tools implicitly, or disable kernel/device protections for a test.

## Support reports

- Run `nacre doctor --report` for Markdown and `nacre doctor --json` for one JSON
  document (schema version 1). Plain `nacre doctor` also emits redacted JSON.
- Open Launcher → Health → **Copy support report**, paste into a local editor and
  compare with the Markdown command. Clipboard failure must show an action error.
- Check distro/kernel, running and installed Hyprland versions, Quickshell/Qt/Python,
  CPU, every GPU and its driver, monitor dimensions/scale/refresh, chassis type,
  battery/backlight/Bluetooth/Wi-Fi presence, audio sinks/sources and dependencies.
- Check recent user-unit warnings/errors (last hour, at most 80 entries), shell log
  tail (60 lines) and compositor configuration errors. No errors and an unavailable
  source are different: unavailable probes have an explicit status.
- Test stopped services, missing utilities, no compositor/session bus, malformed
  release metadata and a timeout. Other report sections must remain usable.
- Inspect both exports for usernames, hostname, home paths, IPv4/IPv6, MACs, Wi-Fi
  names, URL credentials and token-like strings. Seed **synthetic** sensitive errors;
  never use real credentials. Structured metadata is allowlisted; free-form logs
  are scrubbed recursively before either export or clipboard. Review logs before
  sharing: arbitrary prose can include private information without a recognizable
  pattern. Redaction is deliberately conservative (long hashes and four-part
  version numbers can be removed). Reports are local; nothing is uploaded.
- Package checks use the documented Arch runtime package list when pacman responds.
  Other distributions get missing-command checks and an explicit unavailable
  package inventory, rather than Arch packages incorrectly declared missing.

## GPU matrix

Exercise AMD/amdgpu, NVIDIA/nouveau, NVIDIA/proprietary, Intel/i915 and Intel/xe,
including integrated + discrete graphics. Check idle, moderate load and removal or
permission denial of a reading. Compare readings with a trusted vendor tool when
available; record kernel and driver versions with the evidence.

- AMD/nouveau: read GPU-associated hwmon temperature and readable busy percent.
- NVIDIA proprietary: query `nvidia-smi` only if both the driver and executable are
  present. One query covers all GPUs, with a three-second timeout. Unsupported
  values remain unknown. The utility is optional; there is no NVML dependency.
- Intel: read GPU hwmon temperature where exposed, i915 actual frequency
  (`gt_act_freq_mhz` or `gt/gt*/rps_act_freq_mhz`) and xe actual frequency
  (`device/tile*/gt*/freq*/act_freq`), in MHz. Many integrated GPUs expose no
  GPU temperature or global utilization. Only a readable busy-percent counter is
  used for utilization; frequency is never converted into a utilization estimate.
- Inspect resource helper JSON: `gpus` retains each card's identity, vendor, driver,
  temperature, utilization fraction and frequency. Legacy aggregate fields retain
  the maximum **available** reading per metric; they need not come from one card.
  Idle zero is available; unknown is `null`. The percentage gauge hides when its
  utilization is unavailable. Frequency-only devices remain in the helper snapshot.
- Open Today/System, close it, select a different tab, hide the dashboard, and test
  a detached dashboard. Resource sampling runs only while a gauge-bearing view is
  visible. Closing it stops the helper; any vendor command is bounded by its timeout. Fast readings
  run every second and slow sensor readings every 30 seconds while visible.

The sysfs paths follow the [kernel Xe frequency API](https://docs.kernel.org/gpu/xe/xe_gt_freq.html)
and [i915 source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/gpu/drm/i915/i915_sysfs.c).
NVIDIA fields follow its [nvidia-smi reference](https://docs.nvidia.com/deploy/nvidia-smi/index.html).

## Absent devices and mixed displays

- VM/desktop with **no battery**: battery panel explains unavailability; no percentage
  calculation, battery alert or attempted battery write.
- **No backlight** and no DDC device: brightness stays unavailable, opening panels
  issues no brightness writes. Test an external monitor without DDC permissions.
- **No Bluetooth adapter**: settings and popup explain absence, disable power/pairing
  controls, and do not attempt device actions. Repeat with an adapter present but off.
- **Ethernet only**: Wi-Fi settings/popup explain absence and disable the radio.
  The connection editor remains available for wired connections. Repeat with Wi-Fi
  present but disconnected, disabled and blocked; absence and off are distinct.
- **Mixed scales**: connect 2560×1440 at scale 1 and 3840×2160 at scale 1.5, plus an
  internal scale-2 display when possible. Verify report values, independent panel
  placement, correct backlight/DDC ownership and hotplug removal. Scaling must not
  affect which physical display receives a brightness command.
- **No audio devices**: audio controls remain disabled with an unavailable label;
  no null-device errors. Repeat after attaching USB audio and Bluetooth headphones.
- Open every relevant Settings, bar popup, Today/System and lock panel. Watch the
  journal and shell log for QML warnings. Repeat cold login, lock/unlock,
  suspend/resume and device hotplug on physical hardware.

## Automated fixtures and evidence

`nacre/shell-tools/tests/fixtures/hardware/` supplies present/absent hardware and
mixed-scale monitor data. GPU tests build isolated sysfs trees and mock vendor
queries; native Qt tests exercise actual resource, radio, backlight and battery
logic without changing live devices. Run the repository's `python3 tools/check.py`
with the pinned dependencies and native Qt test runner (see contributing docs).

Fixtures verify parsing, availability, timeouts and read-only behavior. They do
not establish sensor accuracy, physical power consumption, driver compatibility,
real login/resume or display placement. Record machine category, exact revision,
redacted report, steps and observed results for each physical acceptance run.
