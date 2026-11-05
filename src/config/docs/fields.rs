//! Field tables for every documented config section.

use super::{FieldDoc, SectionDoc};

/// Config sections in canonical display order.
pub const CONFIG_SECTIONS: &[SectionDoc] = &[
    SectionDoc {
        name: "shell",
        description: "Shell integration settings",
        fields: &[FieldDoc {
            name: "auto_wrap",
            description: "Automatically wrap agent commands for recording",
            default_display: "true",
        }],
    },
    SectionDoc {
        name: "storage",
        description: "Storage and cleanup settings",
        fields: &[
            FieldDoc {
                name: "directory",
                description: "Base directory for session recordings",
                default_display: "~/recorded_agent_sessions",
            },
            FieldDoc {
                name: "size_threshold_gb",
                description: "Warn after recording when storage exceeds this size in GB (unset = off)",
                // ponytail: example value so the commented template stays valid TOML when uncommented
                default_display: "10.0",
            },
            FieldDoc {
                name: "age_threshold_days",
                description: "Age threshold in days for cleanup suggestions",
                default_display: "30",
            },
            FieldDoc {
                name: "compression",
                description: "Compression for new recordings: \"gzip\" (.cast.gz) or \"none\" (.cast)",
                default_display: "gzip",
            },
        ],
    },
    SectionDoc {
        name: "recording",
        description: "Recording behavior settings",
        fields: &[
            FieldDoc {
                name: "auto_analyze",
                description: "Automatically run AI analysis after recording ends",
                default_display: "false",
            },
            FieldDoc {
                name: "filename_template",
                description: "Filename template using {directory}, {?branch}, {id}, {branch}, {date}, {time} tags",
                default_display: "{directory}{?branch}_{id}",
            },
            FieldDoc {
                name: "directory_max_length",
                description: "Maximum characters for directory component in filename",
                default_display: "14",
            },
        ],
    },
    SectionDoc {
        name: "analysis",
        description: "AI analysis settings",
        fields: &[
            FieldDoc {
                name: "agent",
                description: "Preferred agent for analysis (claude, codex, gemini)",
                default_display: "auto-detect",
            },
            FieldDoc {
                name: "workers",
                description: "Number of parallel analysis workers (auto-scale if unset)",
                default_display: "auto",
            },
            FieldDoc {
                name: "timeout",
                description: "Timeout per analysis chunk in seconds",
                default_display: "120",
            },
            FieldDoc {
                name: "fast",
                description: "Fast mode: skip JSON schema enforcement",
                default_display: "false",
            },
            FieldDoc {
                name: "curate",
                description: "Auto-curate markers when count exceeds threshold",
                default_display: "true",
            },
        ],
    },
    SectionDoc {
        name: "agents",
        description: "Agent configuration",
        fields: &[
            FieldDoc {
                name: "enabled",
                description: "List of agents to auto-wrap",
                default_display: r#"["claude", "codex", "gemini"]"#,
            },
            FieldDoc {
                name: "no_wrap",
                description: "Agents to exclude from auto-wrapping",
                default_display: "[]",
            },
        ],
    },
];

/// Per-agent sub-section fields (applies to [agents.claude], [agents.codex], etc.)
pub const AGENT_FIELDS: &[FieldDoc] = &[
    FieldDoc {
        name: "extra_args",
        description: "Default extra CLI arguments for all tasks",
        default_display: "[]",
    },
    FieldDoc {
        name: "analyze_extra_args",
        description: "Extra CLI arguments for analysis (overrides extra_args)",
        default_display: "[]",
    },
    FieldDoc {
        name: "curate_extra_args",
        description: "Extra CLI arguments for curation (overrides extra_args)",
        default_display: "[]",
    },
    FieldDoc {
        name: "rename_extra_args",
        description: "Extra CLI arguments for rename (overrides extra_args)",
        default_display: "[]",
    },
    FieldDoc {
        name: "token_budget",
        description: "Override the token budget for this agent",
        default_display: "auto",
    },
];
