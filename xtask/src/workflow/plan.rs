use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
struct Stage {
    id: String,
    owner: Option<String>,
    files: Vec<String>,
    depends_on: Vec<String>,
}

pub fn validate_plan(path: &Path) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read PLAN file {}", path.display()))?;
    let stages = parse_stages(&content);

    if stages.is_empty() {
        bail!("No stages found in {}", path.display());
    }

    let mut failures = Vec::new();
    let mut by_id = HashMap::new();
    for stage in &stages {
        by_id.insert(stage.id.clone(), stage.clone());
    }

    for stage in &stages {
        if stage.owner.as_deref().unwrap_or("").trim().is_empty() {
            failures.push(format!("{}: missing required field `Owner:`", stage.id));
        }
        if stage.files.is_empty() {
            failures.push(format!(
                "{}: missing or empty required field `Files:`",
                stage.id
            ));
        }
        if stage.depends_on.is_empty() {
            failures.push(format!(
                "{}: missing required field `Depends on:` (use `none` if no dependency)",
                stage.id
            ));
        }
    }

    for stage in &stages {
        for dep in &stage.depends_on {
            if dep == "none" {
                continue;
            }
            if dep == &stage.id {
                failures.push(format!("{}: depends on itself", stage.id));
                continue;
            }
            if !by_id.contains_key(dep) {
                failures.push(format!("{}: unknown dependency `{}`", stage.id, dep));
            }
        }
    }

    if has_cycle(&stages) {
        failures.push("Dependency graph contains a cycle".to_string());
    }

    for i in 0..stages.len() {
        for j in (i + 1)..stages.len() {
            let a = &stages[i];
            let b = &stages[j];

            if depends_on(a, &b.id, &by_id) || depends_on(b, &a.id, &by_id) {
                continue;
            }

            let overlaps: Vec<String> = a
                .files
                .iter()
                .filter(|f| b.files.contains(*f))
                .cloned()
                .collect();

            if !overlaps.is_empty() {
                failures.push(format!(
                    "{} and {} can run in parallel but share files: {}",
                    a.id,
                    b.id,
                    overlaps.join(", ")
                ));
            }
        }
    }

    if failures.is_empty() {
        println!("validate-plan: OK ({})", path.display());
        return Ok(());
    }

    eprintln!("validate-plan: FAILED ({})", path.display());
    for f in failures {
        eprintln!("- {}", f);
    }
    bail!("plan validation failed");
}

pub fn coordinate_plan(path: &Path) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read PLAN file {}", path.display()))?;
    let stages = parse_stages(&content);

    if stages.is_empty() {
        bail!("No stages found in {}", path.display());
    }

    let by_id: HashMap<String, Stage> = stages.iter().cloned().map(|s| (s.id.clone(), s)).collect();

    // Reuse validation before producing schedule.
    validate_plan(path)?;

    let levels = topo_levels(&stages);

    println!("coordinate-plan: {}", path.display());
    println!("Parallel execution batches (dependency-safe):");
    for (idx, level) in levels.iter().enumerate() {
        println!("Batch {}:", idx + 1);
        for stage_id in level {
            if let Some(stage) = by_id.get(stage_id) {
                let owner = stage.owner.clone().unwrap_or_else(|| "unknown".into());
                println!(
                    "- {} | owner={} | files=[{}]",
                    stage.id,
                    owner,
                    stage.files.join(", ")
                );
            }
        }
    }

    println!();
    println!("Per-stage independent review loop:");
    println!("- For each stage PR: spawn Reviewer(Phase=internal)");
    println!("- If internal passes: mark stage PR ready, wait for CodeRabbit");
    println!("- Then spawn Reviewer(Phase=coderabbit) for that same PR");
    println!("- Stage review loops run independently; no cross-stage review dependency");
    println!("- After all stage PRs pass: run one integration review pass before merge");

    Ok(())
}

fn parse_stages(content: &str) -> Vec<Stage> {
    let mut stages = Vec::new();
    let mut current: Option<Stage> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();

        if let Some(stage_id) = parse_stage_header(line) {
            if let Some(stage) = current.take() {
                stages.push(stage);
            }
            current = Some(Stage {
                id: stage_id,
                owner: None,
                files: Vec::new(),
                depends_on: Vec::new(),
            });
            continue;
        }

        let Some(stage) = current.as_mut() else {
            continue;
        };

        if let Some(value) = line.strip_prefix("Owner:") {
            stage.owner = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("Files:") {
            stage.files = parse_files(value);
            continue;
        }
        if let Some(value) = line.strip_prefix("Depends on:") {
            stage.depends_on = parse_depends_on(value);
            continue;
        }
    }

    if let Some(stage) = current.take() {
        stages.push(stage);
    }

    stages
}

fn parse_stage_header(line: &str) -> Option<String> {
    if !line.starts_with("### Stage ") {
        return None;
    }
    let rest = line.trim_start_matches("### ").trim();
    let before_colon = rest.split(':').next()?.trim();
    Some(before_colon.to_string())
}

fn parse_files(value: &str) -> Vec<String> {
    // Primary format: backticked paths in a single line.
    let mut files: Vec<String> = extract_backticked(value);
    if !files.is_empty() {
        return files;
    }

    // Fallback format: comma-separated plain paths.
    files = value
        .split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.trim_matches('`').to_string())
        .collect();
    files
}

fn extract_backticked(value: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut inside = false;
    let mut token = String::new();

    for ch in value.chars() {
        if ch == '`' {
            if inside {
                let cleaned = token.trim();
                if !cleaned.is_empty() {
                    files.push(cleaned.to_string());
                }
                token.clear();
                inside = false;
            } else {
                inside = true;
            }
            continue;
        }
        if inside {
            token.push(ch);
        }
    }
    files
}

fn parse_depends_on(value: &str) -> Vec<String> {
    let raw = value.trim();
    if raw.eq_ignore_ascii_case("none") {
        return vec!["none".to_string()];
    }

    raw.split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| {
            if v.to_lowercase().starts_with("stage ") {
                v.split(':').next().unwrap_or(v).trim().to_string()
            } else {
                v.to_string()
            }
        })
        .collect()
}

fn depends_on(stage: &Stage, target: &str, by_id: &HashMap<String, Stage>) -> bool {
    let mut visited = HashSet::new();
    depends_on_inner(stage, target, by_id, &mut visited)
}

fn depends_on_inner(
    stage: &Stage,
    target: &str,
    by_id: &HashMap<String, Stage>,
    visited: &mut HashSet<String>,
) -> bool {
    if !visited.insert(stage.id.clone()) {
        return false;
    }

    for dep in &stage.depends_on {
        if dep == "none" {
            continue;
        }
        if dep == target {
            return true;
        }
        if let Some(next) = by_id.get(dep) {
            if depends_on_inner(next, target, by_id, visited) {
                return true;
            }
        }
    }
    false
}

fn has_cycle(stages: &[Stage]) -> bool {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for stage in stages {
        let deps: Vec<String> = stage
            .depends_on
            .iter()
            .filter(|d| d.as_str() != "none")
            .cloned()
            .collect();
        graph.insert(stage.id.clone(), deps);
    }

    let mut temp = HashSet::new();
    let mut perm = HashSet::new();

    for node in graph.keys() {
        if visit(node, &graph, &mut temp, &mut perm) {
            return true;
        }
    }
    false
}

fn topo_levels(stages: &[Stage]) -> Vec<Vec<String>> {
    let mut remaining: HashMap<String, HashSet<String>> = HashMap::new();
    for s in stages {
        let deps: HashSet<String> = s
            .depends_on
            .iter()
            .filter(|d| d.as_str() != "none")
            .cloned()
            .collect();
        remaining.insert(s.id.clone(), deps);
    }

    let mut levels = Vec::new();
    while !remaining.is_empty() {
        let ready: Vec<String> = remaining
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(id, _)| id.clone())
            .collect();

        if ready.is_empty() {
            break;
        }

        for id in &ready {
            remaining.remove(id);
        }
        for deps in remaining.values_mut() {
            for id in &ready {
                deps.remove(id);
            }
        }

        levels.push(ready);
    }

    levels
}

fn visit(
    node: &str,
    graph: &HashMap<String, Vec<String>>,
    temp: &mut HashSet<String>,
    perm: &mut HashSet<String>,
) -> bool {
    if perm.contains(node) {
        return false;
    }
    if !temp.insert(node.to_string()) {
        return true;
    }
    if let Some(neighbors) = graph.get(node) {
        for n in neighbors {
            if graph.contains_key(n) && visit(n, graph, temp, perm) {
                return true;
            }
        }
    }
    temp.remove(node);
    perm.insert(node.to_string());
    false
}
