# agr shell

Manage shell integration

## Usage

```
agr shell [OPTIONS]
```

## Description

Manage automatic session recording via shell integration.

Shell integration adds wrapper functions to your shell that automatically
record sessions when you run configured agents. It modifies your .zshrc
or .bashrc with a clearly marked section.

EXAMPLES:
    agr shell status         Check if shell integration is installed
    agr shell install        Install shell integration
    agr shell uninstall      Remove shell integration

After installing, restart your shell or run: source ~/.zshrc

## Subcommands

### shell status

Show shell integration status

Show the current status of shell integration.

Displays whether shell integration is installed, which RC file
is configured, and whether auto-wrap is enabled.

EXAMPLE:
    agr shell status

### shell install

Install shell integration to .zshrc/.bashrc

Install shell integration for automatic session recording.

Adds a clearly marked section to your .zshrc (or .bashrc) that
sources the AGR shell script. This creates wrapper functions for
configured agents that automatically record sessions.

After installation, restart your shell or run:
    source ~/.zshrc

EXAMPLE:
    agr shell install

### shell uninstall

Remove shell integration from .zshrc/.bashrc

Remove shell integration from your shell configuration.

Removes the AGR section from your .zshrc/.bashrc and deletes
the shell script. Restart your shell after uninstalling.

EXAMPLE:
    agr shell uninstall

