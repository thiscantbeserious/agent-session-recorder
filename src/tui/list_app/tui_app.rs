//! `TuiApp` implementation for the list app.

use anyhow::Result;
use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};

use super::render::*;
use super::*;
use crate::files::backup::has_backup;
use crate::tui::app::layout::build_explorer_layout;
use crate::tui::app::list_view::{render_explorer_list, render_explorer_list_with_rename};
use crate::tui::app::status_footer::render_footer_text;
use crate::tui::app::{App, SharedMode, SharedState, TuiApp};
use crate::tui::import;

impl TuiApp for ListApp {
    fn app(&mut self) -> &mut App {
        &mut self.app
    }

    fn shared_state(&mut self) -> &mut SharedState {
        &mut self.shared
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        let (width, height) = self.app.size()?;

        match self.mode {
            Mode::Normal => self.handle_normal_mouse(mouse, height)?,
            Mode::ContextMenu => self.handle_context_menu_mouse(mouse, width, height)?,
            Mode::ConfirmDelete
            | Mode::ConfirmUnlock
            | Mode::ConfirmDeleteFinal
            | Mode::ConfirmUnlockFinal => self.handle_confirm_modal_mouse(mouse, width, height)?,
            // Other modal modes: click outside dismisses
            _ => {
                if let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind {
                    self.mode = Mode::Normal;
                    self.optimize_result = None;
                    self.import_state = None;
                }
            }
        }

        Ok(())
    }

    fn handle_paste(&mut self, text: String) -> Result<()> {
        // Only accept paste events in Normal mode to prevent unexpected mode transitions
        if self.mode != Mode::Normal {
            return Ok(());
        }

        let state = import::ImportState::new(&text, &self.shared.available_agents);
        if !state.has_paths() {
            self.shared.status_message = Some("No file paths found in pasted text".to_string());
            return Ok(());
        }
        self.import_state = Some(state);
        self.mode = Mode::Import;
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
            Mode::ConfirmDelete => self.handle_confirm_delete_key(key)?,
            Mode::ConfirmDeleteFinal => self.handle_confirm_delete_final_key(key)?,
            Mode::ContextMenu => self.handle_context_menu_key(key)?,
            Mode::OptimizeResult => self.handle_optimize_result_key(key)?,
            Mode::ConfirmUnlock => self.handle_confirm_unlock_key(key)?,
            Mode::ConfirmUnlockFinal => self.handle_confirm_unlock_final_key(key)?,
            Mode::RenameInput => self.handle_rename_input_key(key)?,
            Mode::Import => self.handle_import_key(key)?,
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
        let status = self.shared.status_message.clone();
        let agent_filter_idx = self.shared.agent_filter_idx;
        let available_agents = &self.shared.available_agents;
        let context_menu_idx = self.context_menu_idx;
        let optimize_result = self.optimize_result.clone();
        let rename_input = &self.rename_input;
        let rename_cursor = self.rename_cursor;
        let rename_selected_all = self.rename_selected_all;
        let import_state = &self.import_state;

        // Get preview for current selection from cache
        let current_path = explorer.selected_item().map(|i| i.path.clone());
        let preview = current_path
            .as_ref()
            .and_then(|p| self.shared.preview_cache.get(p));

        // Get selected item name for dialog status lines
        let selected_name = explorer
            .selected_item()
            .map(|i| i.name.clone())
            .unwrap_or_default();

        // Check if backup exists for selected file (for context menu)
        let backup_exists = current_path
            .as_ref()
            .map(|p| has_backup(std::path::Path::new(p)))
            .unwrap_or(false);

        self.app.draw(|frame| {
            let area = frame.area();

            // Main layout: explorer + status + footer
            let chunks = build_explorer_layout(area);

            // Render file explorer (no checkboxes in list view - it's single-select)
            if mode == Mode::RenameInput {
                render_explorer_list_with_rename(
                    frame,
                    chunks[0],
                    explorer,
                    preview,
                    false,
                    backup_exists,
                    Some((rename_input, rename_cursor, rename_selected_all)),
                );
            } else {
                render_explorer_list(frame, chunks[0], explorer, preview, false, backup_exists);
            }

            render_status_line_for_mode(
                frame,
                chunks[1],
                mode,
                search_input,
                available_agents,
                agent_filter_idx,
                rename_input,
                rename_cursor,
                rename_selected_all,
                import_state,
                &status,
                explorer,
                &selected_name,
            );

            let footer_text = footer_text_for_mode(mode, import_state);
            render_footer_text(frame, chunks[2], footer_text);

            render_modal_overlays(
                frame,
                area,
                mode,
                explorer,
                context_menu_idx,
                backup_exists,
                &optimize_result,
                import_state,
            );
        })?;

        Ok(())
    }
}
