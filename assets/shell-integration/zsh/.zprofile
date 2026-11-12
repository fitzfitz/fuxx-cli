# fuxx shell integration (zsh). fuxx writes this file at startup; edits are replaced.
#
# Read only with a helper plugin on: fuxx's .zshenv then keeps ZDOTDIR on this folder, so a login
# zsh reads this file. It reads your own .zprofile with your ZDOTDIR, as zsh would, and nothing
# else; then ZDOTDIR is this folder again, so zsh reads fuxx's .zshrc. Everything is quoted, so
# your aliases can't change it.

{
    # Your ZDOTDIR exactly as your .zshenv left it: its value, exported or not, or unset.
    'builtin' 'unset' 'ZDOTDIR'
    if [[ -n "$_fuxx_zdotdir_set" ]]; then
        ZDOTDIR="$_fuxx_zdotdir"
        [[ "$_fuxx_zdotdir_set" != *-export* ]] || 'builtin' 'export' 'ZDOTDIR'
    fi
    # Your .zprofile, as zsh would read it: from ZDOTDIR, else HOME, only if readable.
    'builtin' 'typeset' _fuxx_file="${ZDOTDIR-$HOME}/.zprofile"
    [[ ! -r "$_fuxx_file" ]] || 'builtin' 'source' '--' "$_fuxx_file"
} always {
    # Even if yours failed. Your ZDOTDIR as your .zprofile left it (it may change it), for
    # fuxx's .zshrc; then this folder again, not exported.
    'builtin' 'typeset' _fuxx_zdotdir="${ZDOTDIR-}" _fuxx_zdotdir_set="${(t)ZDOTDIR-}"
    'builtin' 'unset' 'ZDOTDIR' '_fuxx_file'
    ZDOTDIR="$_fuxx_zdotdir_fuxx"
}
