# agr agents

Manage configured agents

## Usage

```
agr agents [OPTIONS]
```

## Description

Manage the list of AI agents that AGR knows about.

Configured agents are used by shell integration to automatically
record sessions. You can also control which agents are auto-wrapped
using the no-wrap subcommand.

EXAMPLES:
    agr agents list                  Show configured agents
    agr agents add claude            Add claude to the list
    agr agents remove codex          Remove codex from the list
    agr agents no-wrap add claude    Disable auto-wrap for claude

## Subcommands

### agents list

List all configured agents

List all agents configured for recording.

These agents can be auto-recorded when shell integration is enabled.

### agents add

Add an agent to the configuration

Add an agent to the configured list.

Once added, the agent can be auto-recorded via shell integration.

EXAMPLE:
    agr agents add claude
    agr agents add my-custom-agent

### agents remove

Remove an agent from the configuration

Remove an agent from the configured list.

The agent will no longer be auto-recorded via shell integration.

EXAMPLE:
    agr agents remove codex

### agents is-wrapped

Check if an agent should be wrapped (used by shell integration)

Check if an agent should be auto-wrapped by shell integration.

Returns exit code 0 if the agent should be wrapped, 1 if not.
Used internally by the shell integration script.

EXAMPLE:
    agr agents is-wrapped claude && echo "Should wrap"

### agents no-wrap

Manage agents that should not be auto-wrapped

Manage the no-wrap list for agents that should not be auto-recorded.

Agents on this list will not be automatically wrapped by shell integration,
even if they are in the configured agents list. Useful for temporarily
disabling recording for specific agents.

