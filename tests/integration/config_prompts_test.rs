//! Tests for the post-migrate prompt registry.

use agr::config::{apply_prompt_list, apply_prompts, migrate_config, PostMigratePrompt};
use anyhow::anyhow;

const V4: &str = "config_version = 4\n\n# keep me\n[recording]\nauto_analyze = false\n";

fn v4_result() -> agr::config::MigrateResult {
    let mut r = migrate_config(V4).unwrap();
    r.added_fields = vec!["recording.auto_optimize".to_string()];
    r
}

#[test]
fn answer_false_writes_false_and_keeps_rest() {
    let r = v4_result();
    let out = apply_prompts(&r, |_| Ok(false)).unwrap();
    assert!(out.contains("auto_optimize = false"), "{out}");
    assert!(out.contains("# keep me"), "{out}");
    assert_eq!(
        out.replace("auto_optimize = false", "auto_optimize = true"),
        r.content
    );
}

#[test]
fn answer_true_keeps_true() {
    let r = v4_result();
    let out = apply_prompts(&r, |_| Ok(true)).unwrap();
    assert!(out.contains("auto_optimize = true"), "{out}");
    assert_eq!(out, r.content);
}

#[test]
fn key_not_added_skips_ask() {
    let mut r = v4_result();
    r.added_fields.clear();
    let mut calls = 0;
    let out = apply_prompts(&r, |_| {
        calls += 1;
        Ok(false)
    })
    .unwrap();
    assert_eq!(calls, 0);
    assert_eq!(out, r.content);
}

#[test]
fn list_asks_in_order_and_writes_both() {
    let mut r = migrate_config("").unwrap();
    r.added_fields = vec![
        "recording.auto_optimize".into(),
        "recording.auto_analyze".into(),
    ];
    let list = [
        PostMigratePrompt {
            key: "recording.auto_optimize",
            question: "one",
            default: true,
        },
        PostMigratePrompt {
            key: "recording.auto_analyze",
            question: "two",
            default: false,
        },
    ];
    let mut asked = Vec::new();
    let out = apply_prompt_list(&r, &list, |p| {
        asked.push(p.question);
        Ok(p.question == "two")
    })
    .unwrap();
    assert_eq!(asked, vec!["one", "two"]);
    assert!(out.contains("auto_optimize = false"), "{out}");
    assert!(out.contains("auto_analyze = true"), "{out}");
}

#[test]
fn ask_error_is_returned() {
    let r = v4_result();
    let res = apply_prompts(&r, |_| Err(anyhow!("boom")));
    assert_eq!(res.map_err(|e| e.to_string()), Err("boom".to_string()));
}

#[test]
fn fresh_config_asks_too() {
    let r = migrate_config("").unwrap();
    assert!(r
        .added_fields
        .contains(&"recording.auto_optimize".to_string()));
    let mut calls = 0;
    let out = apply_prompts(&r, |_| {
        calls += 1;
        Ok(false)
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert!(out.contains("auto_optimize = false"), "{out}");
}
