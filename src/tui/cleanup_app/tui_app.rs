//! `TuiApp` implementation and mouse handling for the cleanup app.

use anyhow::Result;
use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};

use super::status::footer_text_for_mode;
use super::{CleanupApp, Mode};
use crate::tui::app::layout::build_explorer_layout;
use crate::tui::app::list_view::render_explorer_list;
use crate::tui::app::modals;
use crate::tui::app::status_footer::render_footer_text;
use crate::tui::app::{App, SharedMode, SharedState, TuiApp};

impl TuiApp for CleanupApp {
    fn app(&mut self) -> &mut App {
        &mut self.app
    }

    fn shared_state(&mut self) -> &mut SharedState {
        &mut self.shared
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        match self.mode {
            Mode::Normal => {
                let (_, height) = self.app.size()?;
                match mouse.kind {
                    MouseEventKind::ScrollUp => self.shared.explorer.up(),
                    MouseEventKind::ScrollDown => self.shared.explorer.down(),
                    MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                        handle_normal_mouse_click(&mut self.shared, height, mouse.row)?;
                    }
                    _ => {}
                }
            }
            Mode::ConfirmDelete => {
                let (width, height) = self.app.size()?;
                match modals::handle_confirm_click(&mouse, width, height, 50, 8, 6) {
                    modals::ConfirmClick::Yes => {
                        self.mode = Mode::ConfirmDeleteFinal;
                    }
                    modals::ConfirmClick::No => self.mode = Mode::Normal,
                    modals::ConfirmClick::Ignored => {}
                }
            }
            Mode::ConfirmDeleteFinal => {
                let (width, height) = self.app.size()?;
                match modals::handle_confirm_click(&mouse, width, height, 50, 8, 6) {
                    modals::ConfirmClick::Yes => {
                        self.delete_selected()?;
                        self.mode = Mode::Normal;
                    }
                    modals::ConfirmClick::No => self.mode = Mode::Normal,
                    modals::ConfirmClick::Ignored => {}
                }
            }
            // Other modal modes: click outside dismisses
            _ => {
                if let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind {
                    self.mode = Mode::Normal;
                }
            }
        }
        Ok(())
    }

    fn current_shared_mode(&self) -> Option<SharedMode> {
        self.mode.to_shared()
    }

    fn set_mode_from_shared(&mut self, mode: SharedMode) {
        self.mode = Mode::from_shared(mode);
    }

    fn handle_app_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key)?,
            Mode::GlobSelect => self.handle_glob_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_delete_key(key)?,
            Mode::ConfirmDeleteFinal => self.handle_confirm_delete_final_key(key)?,
            _ => {}
        }
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        // Prepare draw: update page size, poll cache, prefetch adjacent previews
        let (_, height) = self.app.size()?;
        self.shared.prepare_draw(height);

        // Extract shared fields into local variables before closure
        let explorer = &mut self.shared.explorer;
        let mode = self.mode;
        let search_input = &self.shared.search_input;
        let glob_input = &self.glob_input;
        let status = self.shared.status_message.clone();
        let agent_filter_idx = self.shared.agent_filter_idx;
        let available_agents = &self.shared.available_agents;

        // Calculate selected size for status bar
        let selected_size: u64 = explorer.selected_items().iter().map(|i| i.size).sum();
        let selected_count = explorer.selected_count();
        let explorer_len = explorer.len();
        let search_filter = explorer.search_filter().map(|s| s.to_string());
        let agent_filter = explorer.agent_filter().map(|s| s.to_string());

        // Get preview for current selection from cache
        let current_path = explorer.selected_item().map(|i| i.path.clone());
        let preview = current_path
            .as_ref()
            .and_then(|p| self.shared.preview_cache.get(p));

        let agent = available_agents[agent_filter_idx].clone();

        self.app.draw(|frame| {
            let area = frame.area();

            // Main layout: explorer + status + footer
            let chunks = build_explorer_layout(area);

            // Render file explorer with checkboxes (cleanup uses multi-select)
            render_explorer_list(frame, chunks[0], explorer, preview, true, false);

            CleanupApp::render_status_for_mode(
                frame,
                chunks[1],
                mode,
                search_input,
                glob_input,
                &agent,
                &status,
                selected_count,
                selected_size,
                explorer_len,
                search_filter.as_deref(),
                agent_filter.as_deref(),
            );

            // Render footer with keybindings
            let footer_text = footer_text_for_mode(mode, selected_count);
            render_footer_text(frame, chunks[2], footer_text);

            // Render modal overlays
            match mode {
                Mode::Help => CleanupApp::render_help_modal(frame, area),
                Mode::ConfirmDelete => {
                    modals::render_confirm_delete_modal(
                        frame,
                        area,
                        selected_count,
                        selected_size,
                        false,
                    );
                }
                Mode::ConfirmDeleteFinal => {
                    modals::render_confirm_delete_modal(
                        frame,
                        area,
                        selected_count,
                        selected_size,
                        true,
                    );
                }
                _ => {}
            }
        })?;

        Ok(())
    }
}

/// Handle a left-click in Normal mode: navigate to the clicked item and toggle its selection.
///
/// Skips locked items (currently being recorded) and shows a status message instead.
fn handle_normal_mouse_click(shared: &mut SharedState, height: u16, click_row: u16) -> Result<()> {
    let explorer_height = height.saturating_sub(2);
    if click_row >= 1 && click_row < explorer_height.saturating_sub(1) {
        let item_offset = (click_row - 1) as usize;
        let scroll_offset = shared.explorer.scroll_offset();
        let visible_idx = scroll_offset + item_offset;
        if shared.explorer.select_index(visible_idx) {
            // Block selection of locked items
            if let Some(item) = shared.explorer.selected_item() {
                if item.lock_info.is_some() {
                    shared.status_message = Some("Session is locked (being recorded)".to_string());
                    return Ok(());
                }
            }
            shared.explorer.toggle_select();
        }
    }
    Ok(())
}
