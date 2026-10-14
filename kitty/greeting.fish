# Loaded after the user's Fish configuration, only in a Nacre terminal.
function fish_greeting
    command nacre-terminal-logo --prepare >/dev/null
    command fastfetch --config "$HOME/.config/nacre/terminal/fastfetch.jsonc"
end
