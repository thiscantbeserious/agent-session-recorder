# agr config

Configuration management

## Usage

```
agr config [OPTIONS]
```

## Description

View and edit the AGR configuration file.

Configuration is stored in ~/.config/agr/config.toml and includes
storage settings, agent list, shell integration options, and more.

EXAMPLES:
    agr config show          Display current configuration
    agr config edit          Open config in $EDITOR

## Subcommands

### config show

Show current configuration as TOML

Display the current configuration in TOML format.

Shows all settings including storage paths, agent list, shell options,
and recording preferences.

EXAMPLE:
    agr config show

### config edit

Open configuration file in your default editor

Open the configuration file in your default editor.

Uses the $EDITOR environment variable (defaults to 'vi').
Config file location: ~/.config/agr/config.toml

EXAMPLE:
    agr config edit
    EDITOR=nano agr config edit

### config migrate

Add missing fields to config file

Add missing fields to your config file.

Scans your config file and adds any fields that exist in the current
version but are missing from your file. Preserves your existing values,
comments, and formatting.

This is useful after upgrading AGR to a new version that introduces
new configuration options. The command shows a preview of changes
and asks for confirmation before writing.

EXAMPLES:
    agr config migrate              Interactive mode (shows preview, asks confirmation)
    agr config migrate --yes        Apply changes without confirmation (for scripts/CI)

### config reset

Reset configuration to defaults

Reset configuration to default values.

Backs up the current config file (if it exists) to config.toml.bak,
then writes a fresh default configuration.

EXAMPLES:
    agr config reset               Interactive mode (asks confirmation)
    agr config reset --yes         Reset without confirmation (for scripts/CI)

