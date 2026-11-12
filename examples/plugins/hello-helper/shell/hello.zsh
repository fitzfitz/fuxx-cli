# Runs once in every new fuxx terminal, after your .zshrc.
typeset -g FUXX_HELLO=1
# FUXX_PLUGIN_DATA is only set while this file is sourced, so keep a copy.
typeset -g FUXX_HELLO_DATA=$FUXX_PLUGIN_DATA
