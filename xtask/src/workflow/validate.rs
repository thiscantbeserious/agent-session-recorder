use std::fs;

use anyhow::{Context, Result};

pub fn run() -> Result<()> {
    let files = [
        "AGENTS.md",
        "agents/skills/roles/SKILL.md",
        "agents/agents/coordinator.md",
        "agents/agents/product-owner.md",
        "agents/agents/architect.md",
        "agents/agents/implementer.md",
        "agents/agents/reviewer-pair.md",
        "agents/agents/reviewer-internal.md",
        "agents/agents/reviewer-coderabbit.md",
        "agents/agents/maintainer.md",
        "agents/skills/roles/references/coordinator.md",
        "agents/skills/roles/references/product-owner.md",
        "agents/skills/roles/references/architect.md",
        "agents/skills/roles/references/implementer.md",
        "agents/skills/roles/references/reviewer.md",
        "agents/skills/roles/references/maintainer.md",
        "agents/skills/instructions/references/state.md",
    ];

    let mut failures: Vec<String> = Vec::new();

    for file in files {
        let content =
            fs::read_to_string(file).with_context(|| format!("Failed to read {}", file))?;

        for banned in [
            "question-and-answer mode",
            ".state/INDEX.md",
            ".state/locks/",
            "PROJECT_DECISIONS.md",
            ".state/decisions.md",
            "references/orchestrator.md",
        ] {
            if content.contains(banned) {
                failures.push(format!("{file}: contains banned reference `{banned}`"));
            }
        }

        if (file.starts_with("agents/skills/roles/") || file.starts_with("agents/agents/"))
            && content.to_lowercase().contains("orchestrator")
        {
            failures.push(format!(
                "{file}: contains legacy term `orchestrator` (expected `coordinator`)"
            ));
        }
    }

    let roles_skill = fs::read_to_string("agents/skills/roles/SKILL.md")
        .context("Failed to read agents/skills/roles/SKILL.md")?;
    if !roles_skill.contains("Direct Assist") {
        failures
            .push("agents/skills/roles/SKILL.md: missing `Direct Assist` startup option".into());
    }
    if !roles_skill.contains("Role-to-Role Collaboration Protocol") {
        failures
            .push("agents/skills/roles/SKILL.md: missing collaboration protocol section".into());
    }
    if !roles_skill
        .contains("Without `/roles`, never start this loop without explicit user confirmation.")
    {
        failures.push(
            "agents/skills/roles/SKILL.md: missing explicit confirmation rule for Direct Assist quick loop".into(),
        );
    }

    let coordinator_ref = fs::read_to_string("agents/skills/roles/references/coordinator.md")
        .context("Failed to read agents/skills/roles/references/coordinator.md")?;
    if !coordinator_ref.contains("Relay and gate only") {
        failures.push(
            "agents/skills/roles/references/coordinator.md: missing relay-and-gate boundary wording"
                .into(),
        );
    }
    if !coordinator_ref
        .contains("must not make domain, requirements, or technical solution decisions")
    {
        failures.push(
            "agents/skills/roles/references/coordinator.md: missing prohibition on domain/solution decisions".into(),
        );
    }
    if !coordinator_ref
        .contains("always requires explicit user confirmation before spawning Implementer/Reviewer")
    {
        failures.push(
            "agents/skills/roles/references/coordinator.md: missing explicit confirmation rule for Direct Assist quick loop".into(),
        );
    }

    let coordinator_agent = fs::read_to_string("agents/agents/coordinator.md")
        .context("Failed to read agents/agents/coordinator.md")?;
    if !coordinator_agent.contains("Task(") {
        failures.push(
            "agents/agents/coordinator.md: missing Task( spawning declaration in frontmatter"
                .into(),
        );
    }

    let state_ref = fs::read_to_string("agents/skills/instructions/references/state.md")
        .context("Failed to read agents/skills/instructions/references/state.md")?;
    if !state_ref.contains("agents/skills/instructions/templates/") {
        failures.push(
            "agents/skills/instructions/references/state.md: wrong templates path (expected agents/skills/instructions/templates/)".into(),
        );
    }

    let plan_template = fs::read_to_string("agents/skills/roles/templates/PLAN.md")
        .context("Failed to read agents/skills/roles/templates/PLAN.md")?;
    for required in ["Owner:", "Depends on:"] {
        if !plan_template.contains(required) {
            failures.push(format!(
                "agents/skills/roles/templates/PLAN.md: missing required field `{required}`"
            ));
        }
    }

    let agent_files = [
        "agents/agents/coordinator.md",
        "agents/agents/product-owner.md",
        "agents/agents/architect.md",
        "agents/agents/implementer.md",
        "agents/agents/reviewer-pair.md",
        "agents/agents/reviewer-internal.md",
        "agents/agents/reviewer-coderabbit.md",
        "agents/agents/maintainer.md",
    ];
    for agent_file in agent_files {
        if !std::path::Path::new(agent_file).exists() {
            failures.push(format!("{agent_file}: agent file does not exist"));
        }
    }

    if failures.is_empty() {
        println!("validate-workflow: OK");
        return Ok(());
    }

    eprintln!("validate-workflow: FAILED");
    for failure in failures {
        eprintln!("- {}", failure);
    }
    anyhow::bail!("workflow validation failed");
}
