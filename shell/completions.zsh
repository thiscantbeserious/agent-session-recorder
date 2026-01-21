#compdef agr

# Zsh completion for AGR (Agent Session Recorder)
# This file should be placed in a directory in your fpath

# Dynamic completion for cast files
_agr_cast_files() {
    local prefix="${words[CURRENT]}"
    local files

    if (( $+commands[agr] )); then
        files=(${(f)"$(agr completions --files "$prefix" 2>/dev/null)"})
        if [[ -n "$files" ]]; then
            _describe -t cast-files 'cast files' files
        fi
    fi
}

# Dynamic completion for agents
_agr_agents() {
    local agents

    if (( $+commands[agr] )); then
        agents=(${(f)"$(agr agents list 2>/dev/null | grep -v '^Configured' | grep -v '^No agents' | sed 's/^  //')"})
        if [[ -z "$agents" ]]; then
            agents=(claude codex gemini-cli)
        fi
        _describe -t agents 'agents' agents
    else
        _values 'agents' claude codex gemini-cli
    fi
}

# Main completion function
_agr() {
    local -a commands
    local curcontext="$curcontext" state line

    commands=(
        'record:Start recording a session'
        'status:Show storage statistics'
        'cleanup:Interactive cleanup of old sessions'
        'list:List recorded sessions'
        'analyze:Analyze a recording with AI'
        'marker:Manage markers in cast files'
        'agents:Manage configured agents'
        'config:Configuration management'
        'shell:Manage shell integration'
    )

    _arguments -C \
        '1: :->command' \
        '*: :->args'

    case "$state" in
        command)
            _describe -t commands 'agr commands' commands
            ;;
        args)
            case "${words[2]}" in
                record)
                    _arguments \
                        '2:agent:_agr_agents' \
                        '--name[Session name]:name:' \
                        '-n[Session name]:name:' \
                        '*:agent args:'
                    ;;
                analyze)
                    _arguments \
                        '2:cast file:_agr_cast_files' \
                        '--agent[Analysis agent]:agent:_agr_agents' \
                        '-a[Analysis agent]:agent:_agr_agents'
                    ;;
                marker)
                    local -a marker_commands
                    marker_commands=(
                        'add:Add a marker to a cast file'
                        'list:List markers in a cast file'
                    )
                    _arguments -C \
                        '2: :->marker_cmd' \
                        '*: :->marker_args'
                    case "$state" in
                        marker_cmd)
                            _describe -t commands 'marker commands' marker_commands
                            ;;
                        marker_args)
                            case "${words[3]}" in
                                add)
                                    _arguments \
                                        '3:cast file:_agr_cast_files' \
                                        '4:timestamp (seconds):' \
                                        '5:label:'
                                    ;;
                                list)
                                    _arguments \
                                        '3:cast file:_agr_cast_files'
                                    ;;
                            esac
                            ;;
                    esac
                    ;;
                list)
                    _arguments \
                        '2:agent:_agr_agents'
                    ;;
                cleanup)
                    _arguments \
                        '--agent[Filter by agent]:agent:_agr_agents' \
                        '--older-than[Only sessions older than N days]:days:(7 14 30 60 90)'
                    ;;
                agents)
                    local -a agent_commands
                    agent_commands=(
                        'list:List configured agents'
                        'add:Add an agent'
                        'remove:Remove an agent'
                        'is-wrapped:Check if agent should be wrapped'
                        'no-wrap:Manage no-wrap list'
                    )
                    _arguments -C \
                        '2: :->agent_cmd' \
                        '*: :->agent_args'
                    case "$state" in
                        agent_cmd)
                            _describe -t commands 'agent commands' agent_commands
                            ;;
                        agent_args)
                            case "${words[3]}" in
                                add|remove|is-wrapped)
                                    _arguments \
                                        '3:agent:_agr_agents'
                                    ;;
                                no-wrap)
                                    local -a nowrap_commands
                                    nowrap_commands=(
                                        'list:List no-wrap agents'
                                        'add:Add to no-wrap list'
                                        'remove:Remove from no-wrap list'
                                    )
                                    _arguments -C \
                                        '3: :->nowrap_cmd' \
                                        '*: :->nowrap_args'
                                    case "$state" in
                                        nowrap_cmd)
                                            _describe -t commands 'no-wrap commands' nowrap_commands
                                            ;;
                                        nowrap_args)
                                            _arguments \
                                                '4:agent:_agr_agents'
                                            ;;
                                    esac
                                    ;;
                            esac
                            ;;
                    esac
                    ;;
                config)
                    local -a config_commands
                    config_commands=(
                        'show:Show current configuration'
                        'edit:Edit configuration file'
                    )
                    _describe -t commands 'config commands' config_commands
                    ;;
                shell)
                    local -a shell_commands
                    shell_commands=(
                        'status:Show shell integration status'
                        'install:Install shell integration'
                        'uninstall:Remove shell integration'
                    )
                    _describe -t commands 'shell commands' shell_commands
                    ;;
            esac
            ;;
    esac
}

_agr "$@"
