#!/usr/bin/env bash
set -euo pipefail
shell_runtime="$HOME/.local/share/nacre/palette-runtime"
unset LD_LIBRARY_PATH QT_PLUGIN_PATH QML_IMPORT_PATH QML2_IMPORT_PATH
export PATH="$HOME/.local/share/nacre/shell/bin:$shell_runtime/venv/bin:$PATH"
case "${1:-start}" in
 menu) exec "$HOME/.local/bin/nacre-menu" "${2:-home}" "${3:-}" ;;
 brightness|keyboard-light)
  target=brightness; [[ "$1" != keyboard-light ]] || target=keyboardLight
  exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call "$target" step "${2:-up}" ;;
 dolphin) exec python3 "$HOME/.local/share/nacre/shell/tools/dolphin-files.py" "${@:2}" ;;
 thunar) exec python3 "$HOME/.local/share/nacre/shell/tools/thunar-files.py" "${@:2}" ;;
 start) exec "$HOME/.local/share/nacre/shell/tools/nacre-helper" shell-supervisor ;;
 close) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre close ;;
 scheme-mode) exec "$HOME/.local/share/nacre/shell/bin/nacre_shell" scheme set -m "${2:-light}" ;;
 wallpaper-image) exec "$HOME/.local/share/nacre/shell/bin/nacre_shell" wallpaper -f "$2" ;;
 wallpaper)
  if [[ $# -gt 1 ]]; then exec "$HOME/.local/share/nacre/shell/bin/nacre_shell" "$@"; fi
  exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre launcher ">wallpaper " ;;
 doctor|profile|check|rollback) exec python3 "$HOME/.local/share/nacre/shell/tools/maintenance.py" "${1/doctor/state}" ;;
 lock-preview) exec "$HOME/.local/share/nacre/shell/bin/qs" -p "$HOME/.local/share/nacre/shell/source/lock-preview.qml" ;;
 detach) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call pearls detach ;;
 pearl) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call pearls open "${2:-launcher}" ;;
 controls) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre controls "${2:-home}" ;;
 settings) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre settings ;;
 left) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre left ;;
 keys) exec "$HOME/.local/bin/nacre-menu" learn.shortcuts ;;
 palette|overview) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre mode "$1" ;;
 clipboard)
  if python3 -c 'import json,pathlib,sys;p=pathlib.Path.home()/".config/nacre/desktop.json";sys.exit(0 if not p.exists() or json.loads(p.read_text()).get("nativeClipboard",True) else 1)';then
   exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre mode clipboard
  else exec "$HOME/.local/bin/nacre-menu" actions.clipboard;fi ;;
 session) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre session ;;
 hide) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre hide ;;
 launcher) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre launcher "" ;;
 toggle) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call drawers toggle dashboard ;;
 apps) exec python3 "$HOME/.local/share/nacre/shell/tools/desktop-actions.py" app-windows "${2:-}" ;;
 updates|wifi|bluetooth|tasks|new|resume|lock|files|terminal|network|audio|volume|focus|workspace|play|next|previous|mute) exec python3 "$HOME/.local/share/nacre/shell/tools/desktop-actions.py" "$1" "${2:-}" ;;
 brain|capture) exec python3 "$HOME/.local/share/nacre/brain/control.py" action "$1" "${2:-}" ;;
 *) exec "$HOME/.local/share/nacre/shell/bin/nacre_shell" "$@" ;;
esac
