//! Unit tests for branding module

use agr::branding::{
    print_box_bottom, print_box_line, print_box_line_end, print_box_prompt, print_done_banner,
    print_full_logo, print_start_banner, truncate_str, BOX_BOTTOM, BOX_WIDTH, LOGO_DONE, LOGO_FULL,
    LOGO_START,
};

#[test]
fn logo_full_is_not_empty() {
    assert!(!LOGO_FULL.is_empty());
    assert!(LOGO_FULL.contains("A G E N T"));
    assert!(LOGO_FULL.contains("R E C O R D E R"));
}

#[test]
fn logo_start_is_not_empty() {
    assert!(!LOGO_START.is_empty());
    assert!(LOGO_START.contains("AGR"));
    assert!(LOGO_START.contains("REC"));
}

#[test]
fn logo_done_is_not_empty() {
    assert!(!LOGO_DONE.is_empty());
    assert!(LOGO_DONE.contains("AGR"));
    assert!(LOGO_DONE.contains("DONE"));
}

#[test]
fn logos_have_box_borders() {
    // Full logo has complete box
    assert!(LOGO_FULL.contains('╔'));
    assert!(LOGO_FULL.contains('╚'));
    // Small logos have top and separator (bottom added programmatically)
    assert!(LOGO_START.contains('╔'));
    assert!(LOGO_START.contains('╠'));
    assert!(LOGO_DONE.contains('╔'));
    assert!(LOGO_DONE.contains('╠'));
}

#[test]
fn truncate_str_short_unchanged() {
    assert_eq!(truncate_str("hello", 10), "hello");
}

#[test]
fn truncate_str_exact_length() {
    assert_eq!(truncate_str("hello", 5), "hello");
}

#[test]
fn truncate_str_adds_ellipsis() {
    assert_eq!(truncate_str("hello world", 8), "hello w…");
}

#[test]
fn truncate_str_handles_unicode_display_width() {
    // CJK characters have display width of 2 each
    // "日本語テスト" = 6 chars, 12 display width
    // With max_width=5, target_width=4, can fit "日本" (width 4) + "..." (width 1) = 5
    assert_eq!(truncate_str("日本語テスト", 5), "日本…");
    // With max_width=7, target_width=6, can fit "日本語" (width 6) + "..." = 7
    assert_eq!(truncate_str("日本語テスト", 7), "日本語…");
}

#[test]
fn truncate_str_handles_wide_emoji() {
    // Emoji typically have display width of 2
    // "hello🎉world" - "hello" (5) + "🎉" (2) + "world" (5) = 12 display width
    assert_eq!(truncate_str("hello🎉world", 8), "hello🎉…");
}

#[test]
fn box_width_matches_bottom_border() {
    // BOX_BOTTOM should be ║ + BOX_WIDTH chars + ║
    // Actually it's ╚ + BOX_WIDTH ═ chars + ╝
    let border_inner: String = BOX_BOTTOM.chars().skip(1).take(BOX_WIDTH).collect();
    assert!(border_inner.chars().all(|c| c == '═'));
}

#[test]
fn print_functions_do_not_panic() {
    // Simple coverage tests - just verify they run without panicking
    print_start_banner();
    print_done_banner();
    print_full_logo();
    print_box_line("test content");
    print_box_line("a very long string that should be truncated to fit the box width");
    print_box_bottom();
    print_box_prompt("prompt: ");
    print_box_line_end();
}
