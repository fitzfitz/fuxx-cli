# fuxx shell integration (zsh). fuxx writes this file at startup; edits are replaced.
#
# Read only with a helper plugin on: fuxx's .zshenv then keeps ZDOTDIR on this folder, so an
# interactive zsh reads this file. It puts your ZDOTDIR back exactly as it was (yours, or unset),
# reads your own .zshrc, then loads the plugins (plugins.zsh, beside this file) at top level, so
# their plain typesets stay global. zsh then reads your .zlogin from your own folder as always.
# Everything is quoted, so your aliases can't change it.

# Before your .zshrc can change anything: plugins.zsh, beside this file (${(%):-%x}, :A:h its
# folder).
'builtin' 'typeset' _fuxx_file="${${(%):-%x}:A:h}/plugins.zsh"
{
    # Your ZDOTDIR exactly as your files left it: its value, exported or not, or unset.
    'builtin' 'unset' 'ZDOTDIR'
    if [[ -n "$_fuxx_zdotdir_set" ]]; then
        ZDOTDIR="$_fuxx_zdotdir"
        [[ "$_fuxx_zdotdir_set" != *-export* ]] || 'builtin' 'export' 'ZDOTDIR'
    fi
    # macOS's /etc/zshrc, read just before this file, set HISTFILE from ZDOTDIR, this folder
    # then: yours instead, as without fuxx.
    [[ "${HISTFILE-}" != "$_fuxx_zdotdir_fuxx/.zsh_history" ]] ||
        HISTFILE="${ZDOTDIR:-$HOME}/.zsh_history"
    # Your .zshrc, as zsh would read it: from ZDOTDIR, else HOME, only if readable.
    'builtin' 'typeset' _fuxx_rc="${ZDOTDIR-$HOME}/.zshrc"
    [[ ! -r "$_fuxx_rc" ]] || 'builtin' 'source' '--' "$_fuxx_rc"
} always {
    {
        # Even if yours failed: the plugins.
        [[ ! -r "$_fuxx_file" ]] || 'builtin' 'source' '--' "$_fuxx_file"
    } always {
        # Even if a plugin failed: nothing of fuxx's is left, and a plugin's folders are set
        # only while its scripts run.
        'builtin' 'unset' '_fuxx_file' '_fuxx_rc' '_fuxx_zdotdir' '_fuxx_zdotdir_set' \
            '_fuxx_zdotdir_fuxx' 'FUXX_PLUGIN_DIR' 'FUXX_PLUGIN_DATA'
    }
}
