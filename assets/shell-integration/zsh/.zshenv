# fuxx shell integration (zsh). fuxx writes this file at startup; edits are replaced.
#
# zsh reads this first because fuxx set ZDOTDIR to this folder. It puts your ZDOTDIR back
# exactly as it was (yours, or unset), reads your own .zshenv, and in an interactive shell
# adds fuxx's hooks. zsh then reads your .zprofile, .zshrc and .zlogin from your own folder as
# always. (Ghostty's and Kitty's method.) Everything is quoted, so your aliases can't change it.

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
    if [[ -o 'interactive' ]]; then
        'builtin' 'typeset' _fuxx_file="${${(%):-%x}:A:h}/fuxx-integration.zsh"
        [[ ! -r "$_fuxx_file" ]] || 'builtin' 'source' '--' "$_fuxx_file"
    fi
    'builtin' 'unset' '_fuxx_file'
}
