//! Mouse handlers for the list app.

use anyhow::Result;
use crossterm::event::{MouseEvent, MouseEventKind};

use super::*;
use crate::tui::app::modals::{self};

impl ListApp {
    /// Handle mouse events in Normal mode.
    ///
    /// Scroll navigates the list; left-click selects and opens context menu or confirm-unlock.
    pub(super) fn handle_normal_mouse(&mut self, mouse: MouseEvent, height: u16) -> Result<()> {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.shared.explorer.up();
            }
            MouseEventKind::ScrollDown => {
                self.shared.explorer.down();
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                let explorer_height = height.saturating_sub(2);
                let click_row = mouse.row;
                if click_row >= 1 && click_row < explorer_height.saturating_sub(1) {
                    let item_offset = (click_row - 1) as usize;
                    let scroll_offset = self.shared.explorer.scroll_offset();
                    let visible_idx = scroll_offset + item_offset;
                    if self.shared.explorer.select_index(visible_idx) {
                        if self.is_selected_locked() {
                            self.mode = Mode::ConfirmUnlock;
                        } else {
                            self.context_menu_idx = 0;
                            self.mode = Mode::ContextMenu;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle mouse events in ContextMenu mode.
    ///
    /// Scroll navigates the menu; left-click either selects an item or dismisses.
    pub(super) fn handle_context_menu_mouse(
        &mut self,
        mouse: MouseEvent,
        width: u16,
        height: u16,
    ) -> Result<()> {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if self.context_menu_idx > 0 {
                    self.context_menu_idx -= 1;
                } else {
                    self.context_menu_idx = ContextMenuItem::ALL.len() - 1;
                }
            }
            MouseEventKind::ScrollDown => {
                self.context_menu_idx = (self.context_menu_idx + 1) % ContextMenuItem::ALL.len();
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                // modal_height = items + 4 (border + title + blank + footer)
                // Items start at row y+3 (border=1, title=1, blank=1)
                let modal_width = 40.min(width.saturating_sub(4));
                let modal_height =
                    (ContextMenuItem::ALL.len() as u16 + 4).min(height.saturating_sub(4));
                let modal_x = (width - modal_width) / 2;
                let modal_y = (height - modal_height) / 2;
                let items_start_y = modal_y + 3;
                let visible_items = modal_height.saturating_sub(4) as usize;

                let cx = mouse.column;
                let cy = mouse.row;

                if cx >= modal_x
                    && cx < modal_x + modal_width
                    && cy >= items_start_y
                    && cy < items_start_y + visible_items as u16
                {
                    let idx = (cy - items_start_y) as usize;
                    if idx < ContextMenuItem::ALL.len() {
                        self.context_menu_idx = idx;
                        self.execute_context_menu_action()?;
                    }
                } else {
                    self.mode = Mode::Normal;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle mouse events in confirm-modal modes (ConfirmDelete/Unlock and Final variants).
    ///
    /// Merges the two first/final confirm arms into one parameterized by current mode.
    pub(super) fn handle_confirm_modal_mouse(
        &mut self,
        mouse: MouseEvent,
        width: u16,
        height: u16,
    ) -> Result<()> {
        let is_first = matches!(self.mode, Mode::ConfirmDelete | Mode::ConfirmUnlock);
        let modal_w = match self.mode {
            Mode::ConfirmDelete | Mode::ConfirmDeleteFinal => 50,
            _ => 55,
        };
        match modals::handle_confirm_click(&mouse, width, height, modal_w, 8, 6) {
            modals::ConfirmClick::Yes => {
                if is_first {
                    self.mode = if self.mode == Mode::ConfirmDelete {
                        Mode::ConfirmDeleteFinal
                    } else {
                        Mode::ConfirmUnlockFinal
                    };
                } else {
                    if self.mode == Mode::ConfirmDeleteFinal {
                        self.delete_session()?;
                    } else {
                        self.remove_selected_lock();
                    }
                    self.mode = Mode::Normal;
                }
            }
            modals::ConfirmClick::No => self.mode = Mode::Normal,
            modals::ConfirmClick::Ignored => {}
        }
        Ok(())
    }
}
