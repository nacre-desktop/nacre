# Name ordinary terminals by location; avoid shell names and host boilerplate.
function fish_title
    set -l folder (path basename "$PWD")
    if test "$PWD" = "$HOME"
        set folder Home
    end
    printf 'Terminal · %s' "$folder"
end
