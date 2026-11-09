# fuxx shell integration (zsh 5.1 or later). fuxx writes this file at startup; edits are
# replaced. Sourced by fuxx's .zshenv in an interactive shell.
#
# Marks each command with the OSC 133 marks fuxx's command blocks read:
#   A  prompt starts   (at the start of PS1)
#   B  prompt ends     (at the end of PS1)
#   C  output starts   (preexec), with the command line percent-encoded as cmdline_url
#   D  command done    (precmd, only after a command ran), with its exit status

# Sourced again (a .zshrc that sources it): everything is in place already.
(( ${+_fuxx_running} )) && 'builtin' 'return' 0

# 1 from a command's start (preexec) until its D is sent.
'builtin' 'typeset' -gi _fuxx_running=0
# 1 once the precmd hook has checked that it runs after the other hooks.
'builtin' 'typeset' -gi _fuxx_ordered=0

# C, with the command line as typed ($1). Sent without it when there is none or it is over
# 2,048 bytes of UTF-8. Encoded byte by byte (multibyte off): everything but A-Z a-z 0-9 . _ ~
# / - becomes %XX, so the mark holds printable ASCII only, and never a `;`.
_fuxx_preexec() {
    'builtin' 'emulate' -L zsh -o extendedglob +o multibyte
    _fuxx_running=1
    if (( ${#1} == 0 || ${#1} > 2048 )); then
        builtin print -rn -- $'\e]133;C\a'
    else
        builtin print -rn -- $'\e]133;C;cmdline_url='"${1//(#m)[^A-Za-z0-9._~\/-]/%${(l:2::0:)$(([##16]#MATCH))}}"$'\a'
    fi
}

# Makes sure PS1 starts with A and ends with B, adding what a prompt theme dropped.
_fuxx_mark_prompt() {
    'builtin' 'emulate' -L zsh -o extendedglob
    local a=$'%{\e]133;A\a%}' b=$'%{\e]133;B\a%}'
    [[ $PS1 == "$a"* ]] || PS1="$a$PS1"
    if [[ $PS1 != *"$b" ]]; then
        # A lone % at the end would read the mark's %{ as %%{, a literal %.
        (( ${#${(M)PS1%%%#}} % 2 )) && PS1+='%'
        PS1+="$b"
    fi
}

# D with the status when a command ran, then the prompt's marks. The first time, it moves
# itself after the other precmd hooks, so a theme's rebuilt PS1 is marked from then on.
_fuxx_precmd() {
    local ret=$? percent=${options[promptpercent]} zle_on=${options[zle]}
    'builtin' 'emulate' -L zsh
    if (( _fuxx_running )); then
        _fuxx_running=0
        builtin print -rn -- $'\e]133;D;'"$ret"$'\a'
    fi
    # Without prompt_percent, %{ %} would show as text.
    [[ $percent == on ]] || return 0
    _fuxx_mark_prompt
    if (( ! _fuxx_ordered )); then
        _fuxx_ordered=1
        if [[ ${precmd_functions[-1]} != _fuxx_precmd ]]; then
            precmd_functions=(${precmd_functions:#_fuxx_precmd} _fuxx_precmd)
            # The hooks after this one still run before this first prompt and may build PS1
            # again: mark it once more when the line editor starts.
            if [[ $zle_on == on ]]; then
                builtin autoload -Uz add-zle-hook-widget
                add-zle-hook-widget line-init _fuxx_line_init
            fi
        fi
    fi
}

# Once, at the first prompt only: redraw it if a later hook left PS1 without its marks. No
# emulate here: the prompt is drawn again with your options (prompt_subst and all).
_fuxx_line_init() {
    add-zle-hook-widget -d line-init _fuxx_line_init
    local before=$PS1
    _fuxx_mark_prompt
    [[ $PS1 == "$before" ]] || zle .reset-prompt
}

'builtin' 'typeset' -ga precmd_functions preexec_functions
precmd_functions+=(_fuxx_precmd)
preexec_functions+=(_fuxx_preexec)
