# Requirements: Play Command

## Problem Statement
Users cannot directly play a specific recording file without navigating through the `ls` interface. Additionally, the native player's powerful capabilities (speed control, navigation shortcuts) are underemphasized in documentation, leaving users unaware of available features.

## User Stories
- As a user, I want to play a recording file directly by path so that I can quickly view a specific session without browsing through listings
- As a new user, I want to see the play command in the quick start guide so that I can immediately understand how to view recordings
- As a user, I want to discover the native player's capabilities (speedup, jump forward/back) in the documentation so that I can efficiently navigate recordings

## Acceptance Criteria
- [ ] `asr play <filepath>` command plays the specified `.cast` file using the native player
- [ ] Running `asr play` without arguments displays usage help
- [ ] Running `asr play` with a non-existent file shows a clear error message
- [ ] README quick start section includes the `play` command with a basic example
- [ ] README documents native player capabilities: speed control, jump forward/back, and any other navigation shortcuts
- [ ] `asr play --help` shows command description and usage

## Out of Scope
- Playing recordings by ID or name (only filepath supported)
- Interactive selection/browsing (use `ls` for that)
- Adding new player features (reuses existing native player as-is)
- Changes to the `ls` command behavior

## Sign-off
- [x] User approved
