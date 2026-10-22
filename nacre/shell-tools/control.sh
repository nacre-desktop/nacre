#!/usr/bin/env bash
set -euo pipefail
shell_runtime="$HOME/.local/share/nacre/palette-runtime"
unset LD_LIBRARY_PATH QT_PLUGIN_PATH QML_IMPORT_PATH QML2_IMPORT_PATH
export PATH="$HOME/.local/share/nacre/shell/bin:$shell_runtime/venv/bin:$PATH"
case "${1:-start}" in
 menu) exec "$HOME/.local/bin/nacre-menu" "${2:-home}" "${3:-}" ;;
 brightness|keyboard-light)
  target=brightness; [[ "$1" != keyboard-light ]] || target=keyboardLight
  exec "$HOME/.local/bin/nacrectl" ui "$target" step "${2:-up}" ;;
 dolphin) exec python3 "$HOME/.local/share/nacre/shell/tools/dolphin-files.py" "${@:2}" ;;
 thunar) exec python3 "$HOME/.local/share/nacre/shell/tools/thunar-files.py" "${@:2}" ;;
 start) exec "$HOME/.local/share/nacre/shell/tools/nacre-helper" shell-supervisor ;;
 close) exec "$HOME/.local/bin/nacrectl" ui nacre close ;;
 scheme-mode) exec "$HOME/.local/share/nacre/shell/bin/nacre_shell" scheme set -m "${2:-light}" ;;
 wallpaper-image) exec "$HOME/.local/share/nacre/shell/bin/nacre_shell" wallpaper -f "$2" ;;
 wallpaper)
  if [[ $# -gt 1 ]]; then exec "$HOME/.local/share/nacre/shell/bin/nacre_shell" "$@"; fi
  exec "$HOME/.local/bin/nacrectl" ui nacre launcher ">wallpaper " ;;
 doctor|profile|check|rollback) exec python3 "$HOME/.local/share/nacre/shell/tools/maintenance.py" "${1/doctor/state}" ;;
 lock-preview) exec "$HOME/.local/share/nacre/shell/bin/qs" -p "$HOME/.local/share/nacre/shell/source/lock-preview.qml" ;;
 detach) exec "$HOME/.local/bin/nacrectl" ui pearls detach ;;
 pearl) exec "$HOME/.local/bin/nacrectl" ui pearls open "${2:-launcher}" ;;
 controls) exec "$HOME/.local/bin/nacrectl" ui nacre controls "${2:-home}" ;;
 settings) exec "$HOME/.local/bin/nacrectl" ui nacre settings ;;
 left) exec "$HOME/.local/bin/nacrectl" ui nacre left ;;
 keys) exec "$HOME/.local/bin/nacre-menu" learn.shortcuts ;;
 palette|overview) exec "$HOME/.local/bin/nacrectl" ui nacre mode "$1" ;;
 clipboard) exec "$HOME/.local/bin/nacrectl" ui nacre clipboard ;;
 session) exec "$HOME/.local/bin/nacrectl" ui nacre session ;;
 hide) exec "$HOME/.local/bin/nacrectl" ui nacre hide ;;
 launcher) exec "$HOME/.local/bin/nacrectl" ui nacre launcher "" ;;
 toggle) exec "$HOME/.local/bin/nacrectl" ui drawers toggle dashboard ;;
 apps) exec "$HOME/.local/share/nacre/shell/tools/nacre-helper" desktop-actions app-windows "${2:-}" ;;
 updates|wifi|bluetooth|tasks|new|resume|lock|files|terminal|network|audio|volume|focus|workspace|play|next|previous|mute) exec "$HOME/.local/share/nacre/shell/tools/nacre-helper" desktop-actions "$1" "${2:-}" ;;
 brain|capture) exec python3 "$HOME/.local/share/nacre/brain/control.py" action "$1" "${2:-}" ;;
 *) exec "$HOME/.local/share/nacre/shell/bin/nacre_shell" "$@" ;;
esac
