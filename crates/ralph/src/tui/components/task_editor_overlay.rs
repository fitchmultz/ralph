//! Task editor overlay component using the foundation layer.
//!
//! Responsibilities:
//! - Render the task editor overlay with focus-managed field list and textarea.
//! - Handle Tab/Shift-Tab navigation between focusable elements.
//! - Integrate with the existing AppMode::EditingTask state.
//!
//! Not handled here:
//! - Task edit persistence (handled by App::apply_task_edit).
//! - Text input handling (delegated to MultiLineInput).
//!
//! Invariants/assumptions:
//! - The component is only rendered when AppMode::EditingTask is active.
//! - Focus scope is managed externally (enter_overlay_scope on open, exit on close).
//! - The textarea is only focusable when editing_value is Some.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::outpututil::truncate_chars;
use crate::tui::{
    App, MultiLineInput, TaskEditKind,
    foundation::{
        Component, ComponentId, FocusId, FocusManager, Item, ItemSize, RenderCtx, UiEvent,
        centered, col,
    },
};

/// Messages produced by the task editor overlay component.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TaskEditorMessage {
    /// Navigate to the previous field.
    NavigateUp,
    /// Navigate to the next field.
    NavigateDown,
    /// Start editing the current field.
    StartEdit,
    /// Commit the current edit with the given value.
    CommitEdit(String),
    /// Cancel the current edit.
    CancelEdit,
    /// Clear the current field.
    ClearField,
    /// Close the overlay.
    Close,
    /// No action needed.
    Noop,
}

/// Component for the task editor overlay.
///
/// This component manages the rendering and event handling for the task
/// editor modal, including focus management between the field list and
/// the textarea when editing.
pub(crate) struct TaskEditorOverlayComponent {
    /// Component ID for focus registration.
    id: ComponentId,
    /// Currently selected field index.
    selected: usize,
    /// Whether we're currently in edit mode (textarea active).
    editing_value: Option<MultiLineInput>,
    /// Whether the textarea is focused.
    textarea_focused: bool,
}

impl TaskEditorOverlayComponent {
    /// Create a new task editor overlay component.
    pub(crate) fn new(selected: usize, editing_value: Option<MultiLineInput>) -> Self {
        let textarea_focused = editing_value.is_some();
        Self {
            id: ComponentId::new("task_editor_overlay", 0),
            selected,
            editing_value,
            textarea_focused,
        }
    }

    /// Get the currently selected field index.
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// Check if we're currently editing.
    pub(crate) fn is_editing(&self) -> bool {
        self.editing_value.is_some()
    }

    /// Get a reference to the editing value.
    pub(crate) fn editing_value(&self) -> Option<&MultiLineInput> {
        self.editing_value.as_ref()
    }

    /// Set the editing value.
    pub(crate) fn set_editing_value(&mut self, value: Option<MultiLineInput>) {
        self.textarea_focused = value.is_some();
        self.editing_value = value;
    }

    /// Get focus IDs for this component's focusable elements.
    fn field_list_focus_id(&self) -> FocusId {
        FocusId::new(self.id, 0)
    }

    fn textarea_focus_id(&self) -> FocusId {
        FocusId::new(self.id, 1)
    }

    /// Render the field list.
    fn render_field_list(&self, f: &mut Frame<'_>, area: Rect, app: &App, ctx: &mut RenderCtx<'_>) {
        let entries = app.task_edit_entries();
        if entries.is_empty() {
            return;
        }

        let label_width = 18usize;
        let is_editing = self.editing_value.is_some();

        let items: Vec<ListItem> = entries
            .iter()
            .enumerate()
            .take(area.height as usize)
            .map(|(idx, entry)| {
                let is_selected = idx == self.selected;
                let value = if is_selected && is_editing {
                    "...".to_string()
                } else {
                    entry.value.clone()
                };
                let label = format!("{:label_width$}", entry.label);
                let line_text = format!("{} {}", label, value);
                let display = truncate_chars(&line_text, area.width as usize);

                let mut style = Style::default();
                if entry.value == "(empty)" {
                    style = style.fg(Color::DarkGray);
                }
                if is_selected {
                    style = style.bg(Color::Blue).add_modifier(Modifier::BOLD);
                }

                ListItem::new(Line::from(Span::styled(display, style)))
            })
            .collect();

        let list = List::new(items).block(Block::default());
        f.render_widget(list, area);

        // Register focus node for the field list
        let is_enabled = !is_editing || !self.textarea_focused;
        ctx.register_focus(self.field_list_focus_id(), area, is_enabled);
    }

    /// Render the textarea overlay when editing.
    fn render_textarea(&self, f: &mut Frame<'_>, popup_area: Rect, ctx: &mut RenderCtx<'_>) {
        if let Some(textarea) = &self.editing_value {
            let edit_area = Rect {
                x: popup_area.x + 2,
                y: popup_area.y + 2 + self.selected as u16,
                width: popup_area.width.saturating_sub(4),
                height: 6.min(popup_area.height.saturating_sub(4)),
            };
            f.render_widget(textarea.widget(), edit_area);

            // Register focus node for the textarea
            ctx.register_focus(self.textarea_focus_id(), edit_area, true);
        }
    }

    /// Render the hint text at the bottom.
    fn render_hint(&self, f: &mut Frame<'_>, area: Rect) {
        let is_editing = self.editing_value.is_some();

        let hint = if is_editing {
            Line::from(vec![
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":commit "),
                Span::styled("Alt+Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":newline "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":cancel"),
            ])
        } else {
            Line::from(vec![
                Span::styled("Enter/Space", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":edit "),
                Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":nav "),
                Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":clear "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(":close"),
            ])
        };

        let format_hint = Line::from(vec![
            Span::styled("lists", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": one item per line  "),
            Span::styled("maps", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(": key=value"),
        ]);

        let hint_paragraph = Paragraph::new(ratatui::text::Text::from(vec![hint, format_hint]))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));

        f.render_widget(hint_paragraph, area);
    }
}

impl Component for TaskEditorOverlayComponent {
    type Message = TaskEditorMessage;

    fn render(&mut self, f: &mut Frame<'_>, area: Rect, app: &App, ctx: &mut RenderCtx<'_>) {
        let entries = app.task_edit_entries();
        if entries.is_empty() {
            return;
        }

        // Calculate popup dimensions
        let popup_width = 96.min(area.width.saturating_sub(4)).max(44);
        let popup_height = (entries.len() as u16 + 7)
            .min(area.height.saturating_sub(4))
            .max(9);

        let popup_area = centered(area, popup_width, popup_height);

        // Clear background
        f.render_widget(Clear, popup_area);

        // Draw border
        let title = Line::from(vec![Span::styled(
            "Task Editor",
            Style::default().add_modifier(Modifier::BOLD),
        )]);
        let block = Block::default().borders(Borders::ALL).title(title);
        f.render_widget(block.clone(), popup_area);

        // Get inner area
        let inner = block.inner(popup_area);

        // Split into list and hint areas
        let layout = col(
            inner,
            0,
            &[Item::new(ItemSize::Min(1)), Item::new(ItemSize::Fixed(2))],
        );

        let list_area = layout[0];
        let hint_area = layout[1];

        // Render components
        self.render_field_list(f, list_area, app, ctx);
        self.render_textarea(f, popup_area, ctx);
        self.render_hint(f, hint_area);
    }

    fn handle_event(
        &mut self,
        event: &UiEvent,
        app: &App,
        focus: &mut FocusManager,
    ) -> Option<Self::Message> {
        let entries = app.task_edit_entries();
        if entries.is_empty() {
            return Some(TaskEditorMessage::Close);
        }

        let max_index = entries.len().saturating_sub(1);

        // Handle editing mode first - take ownership of the textarea temporarily
        if self.editing_value.is_some() {
            return self.handle_editing_event(event, app, max_index);
        }

        // Handle navigation mode
        self.handle_navigation_event(event, app, focus, max_index)
    }

    fn focus_gained(&mut self) {
        // Reset focus to field list when overlay gains focus
        self.textarea_focused = false;
    }

    fn focus_lost(&mut self) {
        // Cancel any active edit when focus is lost
        if self.editing_value.is_some() {
            self.editing_value = None;
            self.textarea_focused = false;
        }
    }
}

impl TaskEditorOverlayComponent {
    /// Handle events when in editing mode (textarea active).
    fn handle_editing_event(
        &mut self,
        event: &UiEvent,
        app: &App,
        max_index: usize,
    ) -> Option<TaskEditorMessage> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Take ownership of the textarea temporarily
        let mut textarea = self.editing_value.take()?;

        match event {
            UiEvent::Key(key) => match key.code {
                KeyCode::Enter => {
                    // Check if Alt is pressed - if so, insert newline
                    if key.modifiers.contains(KeyModifiers::ALT) {
                        textarea.input(*key);
                        self.editing_value = Some(textarea);
                        Some(TaskEditorMessage::Noop)
                    } else {
                        // Commit the edit
                        let entry = &app.task_edit_entries()[self.selected.min(max_index)];
                        let edit_value = if app.is_list_field(entry.key) {
                            textarea.lines().join(", ")
                        } else {
                            textarea.value().to_string()
                        };
                        // Don't restore textarea - editing is done
                        self.textarea_focused = false;
                        Some(TaskEditorMessage::CommitEdit(edit_value))
                    }
                }
                KeyCode::Esc => {
                    // Cancel - don't restore textarea
                    self.textarea_focused = false;
                    Some(TaskEditorMessage::CancelEdit)
                }
                _ => {
                    // Pass other keys to textarea
                    textarea.input(*key);
                    self.editing_value = Some(textarea);
                    Some(TaskEditorMessage::Noop)
                }
            },
            _ => {
                // Restore textarea for other events
                self.editing_value = Some(textarea);
                Some(TaskEditorMessage::Noop)
            }
        }
    }

    /// Handle events when in navigation mode (field list active).
    fn handle_navigation_event(
        &mut self,
        event: &UiEvent,
        app: &App,
        focus: &mut FocusManager,
        max_index: usize,
    ) -> Option<TaskEditorMessage> {
        // Check for Tab/Shift-Tab to cycle focus
        if event.is_tab() {
            focus.focus_next();
            self.update_focus_state(focus);
            return Some(TaskEditorMessage::Noop);
        }

        if event.is_backtab() {
            focus.focus_prev();
            self.update_focus_state(focus);
            return Some(TaskEditorMessage::Noop);
        }

        // Handle other navigation keys
        if event.is_escape() {
            return Some(TaskEditorMessage::Close);
        }

        if event.is_up() || event.is_plain_char('k') {
            self.selected = self.selected.saturating_sub(1);
            return Some(TaskEditorMessage::NavigateUp);
        }

        if event.is_down() || event.is_plain_char('j') {
            self.selected = (self.selected + 1).min(max_index);
            return Some(TaskEditorMessage::NavigateDown);
        }

        if event.is_enter() || event.is_plain_char(' ') {
            let entry = &app.task_edit_entries()[self.selected.min(max_index)];
            match entry.kind {
                TaskEditKind::Cycle => {
                    // For cycle fields, just trigger the cycle
                    return Some(TaskEditorMessage::CommitEdit("".to_string()));
                }
                TaskEditKind::Text
                | TaskEditKind::List
                | TaskEditKind::Map
                | TaskEditKind::OptionalText => {
                    return Some(TaskEditorMessage::StartEdit);
                }
            }
        }

        if event.is_plain_char('x') {
            return Some(TaskEditorMessage::ClearField);
        }

        // Handle typing to start edit
        if let Some(_ch) = event.char() {
            let entry = &app.task_edit_entries()[self.selected.min(max_index)];
            match entry.kind {
                TaskEditKind::Text
                | TaskEditKind::List
                | TaskEditKind::Map
                | TaskEditKind::OptionalText => {
                    return Some(TaskEditorMessage::StartEdit);
                }
                TaskEditKind::Cycle => {}
            }
        }

        Some(TaskEditorMessage::Noop)
    }

    /// Update internal focus state based on focus manager.
    fn update_focus_state(&mut self, focus: &FocusManager) {
        let focused_id = focus.focused();
        self.textarea_focused = focused_id == Some(self.textarea_focus_id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_creation() {
        let component = TaskEditorOverlayComponent::new(0, None);
        assert_eq!(component.selected(), 0);
        assert!(!component.is_editing());
    }

    #[test]
    fn test_component_with_editing() {
        let textarea = MultiLineInput::new("test".to_string(), false);
        let component = TaskEditorOverlayComponent::new(1, Some(textarea));
        assert_eq!(component.selected(), 1);
        assert!(component.is_editing());
    }

    #[test]
    fn test_focus_ids() {
        let component = TaskEditorOverlayComponent::new(0, None);
        let field_list_id = component.field_list_focus_id();
        let textarea_id = component.textarea_focus_id();

        assert_eq!(field_list_id.component.kind, "task_editor_overlay");
        assert_eq!(field_list_id.local, 0);
        assert_eq!(textarea_id.local, 1);
    }
}
