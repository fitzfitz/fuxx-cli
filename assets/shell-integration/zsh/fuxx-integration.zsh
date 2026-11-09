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
# 1 once the line-init hook is added (at the first prompt, when the line editor is on).
'builtin' 'typeset' -gi _fuxx_hooked=0
# Where the marks go: the terminal itself, so a redirected or closed stdout (`exec >file`)
# neither gets them nor loses them. Stdout if /dev/tty can't be opened. Not passed on to
# programs the shell runs (cloexec).
'builtin' 'typeset' -gi _fuxx_fd=1
{ 'builtin' 'zmodload' zsh/system && 'builtin' 'sysopen' -wu _fuxx_fd -o cloexec /dev/tty } 2>/dev/null ||
    _fuxx_fd=1

# C, with the command line as typed ($1). Sent without it when there is none or it is over
# 2,048 bytes of UTF-8. Encoded byte by byte (multibyte off): everything but A-Z a-z 0-9 . _ ~
# / - becomes %XX, so the mark holds printable ASCII only, and never a `;`.
_fuxx_preexec() {
    'builtin' 'emulate' -L zsh -o extendedglob +o multibyte
    _fuxx_running=1
    if (( ${#1} == 0 || ${#1} > 2048 )); then
        builtin print -rnu $_fuxx_fd -- $'\e]133;C\a'
    else
        builtin print -rnu $_fuxx_fd -- $'\e]133;C;cmdline_url='"${1//(#m)[^A-Za-z0-9._~\/-]/%${(l:2::0:)$(([##16]#MATCH))}}"$'\a'
    fi
}

# Makes sure PS1 starts with A and ends with B, once each: adds what a prompt theme dropped,
# and drops copies a theme kept inside it (pure keeps what follows its prompt's last newline,
# and with it an A this hook had put there).
_fuxx_mark_prompt() {
    'builtin' 'emulate' -L zsh -o extendedglob
    local a=$'%{\e]133;A\a%}' b=$'%{\e]133;B\a%}'
    [[ $PS1 == "$a"* && ${PS1#"$a"} != *"$a"* ]] || PS1="$a${PS1//"$a"/}"
    if [[ $PS1 != *"$b" || ${PS1%"$b"} == *"$b"* ]]; then
        PS1=${PS1//"$b"/}
        # A lone % at the end would read the mark's %{ as %%{, a literal %.
        (( ${#${(M)PS1%%%#}} % 2 )) && PS1+='%'
        PS1+="$b"
    fi
}

# D with the status when a command ran, then the prompt's marks. It moves itself after the other
# precmd hooks, so a theme's rebuilt PS1 is marked; for a theme that moves its own hook last at
# every prompt too (powerlevel10k), the line-init hook below moves it back once the prompt is up.
_fuxx_precmd() {
    local ret=$? percent=${options[promptpercent]} zle_on=${options[zle]}
    'builtin' 'emulate' -L zsh
    if (( _fuxx_running )); then
        _fuxx_running=0
        builtin print -rnu $_fuxx_fd -- $'\e]133;D;'"$ret"$'\a'
    fi
    # powerlevel10k builds PS1 again where no hook can mark it (its transient prompt, Ctrl-C,
    # instant prompt): have it put A and B in every prompt it builds, as it does for kitty and
    # Warp. Not if you set this yourself. It reads it at its first prompt, after this hook.
    (( ! $+functions[p10k] || $+POWERLEVEL9K_TERM_SHELL_INTEGRATION )) ||
        'builtin' 'typeset' -g POWERLEVEL9K_TERM_SHELL_INTEGRATION=true
    # Without prompt_percent, %{ %} would show as text.
    [[ $percent == on ]] || return 0
    _fuxx_p10k_marks || _fuxx_mark_prompt
    [[ ${precmd_functions[-1]} == _fuxx_precmd ]] ||
        precmd_functions=(${precmd_functions:#_fuxx_precmd} _fuxx_precmd)
    if (( ! _fuxx_hooked )) && [[ $zle_on == on ]]; then
        _fuxx_hooked=1
        builtin autoload -Uz add-zle-hook-widget
        add-zle-hook-widget line-init _fuxx_line_init
    fi
}

# Whether powerlevel10k puts A and B in its prompts itself. Its own C and D are then turned
# off: it sends them for an empty line too, so Return or Ctrl-C at a prompt would be a block.
_fuxx_p10k_marks() {
    (( ${_POWERLEVEL9K_TERM_SHELL_INTEGRATION:-0} )) || return 1
    (( ! $+_p9k__iterm_cmd )) || unset _p9k__iterm_cmd
}

# At every prompt, once it is up: put _fuxx_precmd last again (a theme's hook may have moved
# itself after it), and if a hook that ran after it built PS1 again, mark it and redraw. No
# emulate here: the prompt is drawn again with your options (prompt_subst and all).
_fuxx_line_init() {
    _fuxx_p10k_marks && return 0
    [[ ${precmd_functions[-1]} == _fuxx_precmd ]] ||
        precmd_functions=(${precmd_functions:#_fuxx_precmd} _fuxx_precmd)
    [[ ${options[promptpercent]} == on ]] || return 0
    local before=$PS1
    _fuxx_mark_prompt
    [[ $PS1 == "$before" ]] || zle .reset-prompt
}

'builtin' 'typeset' -ga precmd_functions preexec_functions
precmd_functions+=(_fuxx_precmd)
preexec_functions+=(_fuxx_preexec)
