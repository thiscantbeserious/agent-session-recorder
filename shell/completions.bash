# Bash completion for AGR (Agent Session Recorder)
# This file is sourced by agr.sh to provide tab-completion

# Dynamic completion for cast files
_agr_complete_files() {
    local prefix="${COMP_WORDS[COMP_CWORD]}"

    # Get list of cast files from agr
    if command -v agr &>/dev/null; then
        local files
        files=$(agr completions --files "$prefix" 2>/dev/null)
        if [[ -n "$files" ]]; then
            COMPREPLY=( $(compgen -W "$files" -- "$prefix") )
        fi
    fi
}

# Main completion function for agr
_agr_completions() {
    local cur prev words cword
    _get_comp_words_by_ref -n : cur prev words cword 2>/dev/null || {
        cur="${COMP_WORDS[COMP_CWORD]}"
        prev="${COMP_WORDS[COMP_CWORD-1]}"
        words=("${COMP_WORDS[@]}")
        cword=$COMP_CWORD
    }

    # Top-level commands
    local commands="record status cleanup list analyze marker agents config shell"

    # Determine context based on position and previous words
    case "$cword" in
        1)
            # First argument: complete commands
            COMPREPLY=( $(compgen -W "$commands" -- "$cur") )
            return
            ;;
    esac

    # Context-specific completions
    case "${words[1]}" in
        analyze)
            # After 'analyze', complete with cast files or --agent flag
            case "$prev" in
                analyze)
                    _agr_complete_files
                    COMPREPLY+=( $(compgen -W "--agent -a" -- "$cur") )
                    ;;
                --agent|-a)
                    # Complete with known agents
                    local agents
                    agents=$(agr agents list 2>/dev/null | grep -v "^Configured" | grep -v "^No agents" | sed 's/^  //')
                    COMPREPLY=( $(compgen -W "$agents claude codex gemini" -- "$cur") )
                    ;;
                *)
                    # If we haven't specified a file yet, offer files and flags
                    if [[ ! " ${words[*]} " =~ " --agent " ]] && [[ ! " ${words[*]} " =~ " -a " ]]; then
                        COMPREPLY=( $(compgen -W "--agent -a" -- "$cur") )
                    fi
                    ;;
            esac
            ;;
        marker)
            case "$prev" in
                marker)
                    # After 'marker', complete with subcommands
                    COMPREPLY=( $(compgen -W "add list" -- "$cur") )
                    ;;
                add|list)
                    # After 'add' or 'list', complete with cast files
                    _agr_complete_files
                    ;;
            esac
            ;;
        record)
            case "$prev" in
                record)
                    # After 'record', complete with agents
                    local agents
                    agents=$(agr agents list 2>/dev/null | grep -v "^Configured" | grep -v "^No agents" | sed 's/^  //')
                    COMPREPLY=( $(compgen -W "$agents claude codex gemini" -- "$cur") )
                    ;;
                *)
                    # Offer --name flag
                    COMPREPLY=( $(compgen -W "--name -n" -- "$cur") )
                    ;;
            esac
            ;;
        list)
            case "$prev" in
                list)
                    # After 'list', complete with agents
                    local agents
                    agents=$(agr agents list 2>/dev/null | grep -v "^Configured" | grep -v "^No agents" | sed 's/^  //')
                    COMPREPLY=( $(compgen -W "$agents" -- "$cur") )
                    ;;
            esac
            ;;
        cleanup)
            case "$prev" in
                --agent)
                    # Complete with agents
                    local agents
                    agents=$(agr agents list 2>/dev/null | grep -v "^Configured" | grep -v "^No agents" | sed 's/^  //')
                    COMPREPLY=( $(compgen -W "$agents" -- "$cur") )
                    ;;
                --older-than)
                    # Suggest common day values
                    COMPREPLY=( $(compgen -W "7 14 30 60 90" -- "$cur") )
                    ;;
                *)
                    COMPREPLY=( $(compgen -W "--agent --older-than" -- "$cur") )
                    ;;
            esac
            ;;
        agents)
            case "$prev" in
                agents)
                    COMPREPLY=( $(compgen -W "list add remove is-wrapped no-wrap" -- "$cur") )
                    ;;
                add|remove|is-wrapped)
                    local agents
                    agents=$(agr agents list 2>/dev/null | grep -v "^Configured" | grep -v "^No agents" | sed 's/^  //')
                    COMPREPLY=( $(compgen -W "$agents claude codex gemini" -- "$cur") )
                    ;;
                no-wrap)
                    COMPREPLY=( $(compgen -W "list add remove" -- "$cur") )
                    ;;
            esac
            ;;
        config)
            case "$prev" in
                config)
                    COMPREPLY=( $(compgen -W "show edit" -- "$cur") )
                    ;;
            esac
            ;;
        shell)
            case "$prev" in
                shell)
                    COMPREPLY=( $(compgen -W "status install uninstall" -- "$cur") )
                    ;;
            esac
            ;;
    esac
}

# Register the completion function
complete -F _agr_completions agr
