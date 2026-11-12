# fuxx shell integration (zsh). fuxx writes this file at startup; edits are replaced.
#
# zsh reads this first because fuxx set ZDOTDIR to this folder. It puts your ZDOTDIR back
# exactly as it was (yours, or unset), reads your own .zshenv, and in an interactive shell
# adds fuxx's hooks (FUXX_ZSH_MARKS, command blocks). zsh then reads your .zprofile, .zshrc and
# .zlogin from your own folder as always. (Ghostty's and Kitty's method.)
#
# With a helper plugin on (FUXX_ZSH_PLUGINS) an interactive shell instead keeps ZDOTDIR on this
# folder after your .zshenv, so zsh reads fuxx's .zprofile and .zshrc, which read yours with your
# ZDOTDIR and then load the plugins. Everything is quoted, so your aliases can't change it.

# Read once and unset, so a shell started from this one starts clean. Neither set: an older
# fuxx (before plugins) set ZDOTDIR alone, for command blocks.
'builtin' 'typeset' _fuxx_marks="${FUXX_ZSH_MARKS-}" _fuxx_plugins="${FUXX_ZSH_PLUGINS-}"
[[ -n "$_fuxx_marks$_fuxx_plugins" ]] || _fuxx_marks=1
'builtin' 'typeset' _fuxx_zdotdir_fuxx="$ZDOTDIR"
'builtin' 'unset' 'FUXX_ZSH_MARKS' 'FUXX_ZSH_PLUGINS'

if [[ -n "${FUXX_ZSH_ZDOTDIR+X}" ]]; then
    'builtin' 'export' ZDOTDIR="$FUXX_ZSH_ZDOTDIR"
    'builtin' 'unset' 'FUXX_ZSH_ZDOTDIR'
else
    'builtin' 'unset' 'ZDOTDIR'
fi

{
    # Your .zshenv, as zsh would read it: from ZDOTDIR, else HOME, only if readable.
    'builtin' 'typeset' _fuxx_file="${ZDOTDIR-$HOME}/.zshenv"
    [[ ! -r "$_fuxx_file" ]] || 'builtin' 'source' '--' "$_fuxx_file"
} always {
    # Even if yours failed. ${(%):-%x} is this file, :A:h its folder.
    if [[ -o 'interactive' && -n "$_fuxx_marks" ]]; then
        'builtin' 'typeset' _fuxx_file="${${(%):-%x}:A:h}/fuxx-integration.zsh"
        [[ ! -r "$_fuxx_file" ]] || 'builtin' 'source' '--' "$_fuxx_file"
    fi
    if [[ -o 'interactive' && -n "$_fuxx_plugins" ]]; then
        # Your ZDOTDIR as your .zshenv left it (it may set it): its value, and its type (empty
        # when unset, so set-and-empty differs). fuxx's .zprofile and .zshrc put it back.
        'builtin' 'typeset' _fuxx_zdotdir="${ZDOTDIR-}" _fuxx_zdotdir_set="${(t)ZDOTDIR-}"
        # This folder, not exported, so no program started meanwhile sees it.
        'builtin' 'unset' 'ZDOTDIR'
        ZDOTDIR="$_fuxx_zdotdir_fuxx"
    else
        'builtin' 'unset' '_fuxx_zdotdir_fuxx'
    fi
    'builtin' 'unset' '_fuxx_file' '_fuxx_marks' '_fuxx_plugins'
}
