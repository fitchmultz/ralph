use super::super::{App, AppMode};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Draw the footer area.
pub(super) fn draw_footer(f: &mut Frame<'_>, app: &App, area: Rect) {
    let help_text = help_footer_spans(app);

    let help_paragraph = Paragraph::new(Line::from(help_text))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    f.render_widget(help_paragraph, area);
}

pub(super) fn help_footer_spans(app: &App) -> Vec<Span<'static>> {
    let mut help_text = match &app.mode {
        AppMode::Normal => vec![
            Span::styled("?/h", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":help "),
            Span::styled(":", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cmd "),
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":quit "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":nav "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":run "),
            Span::styled("d", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":del "),
            Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":edit "),
            Span::styled("/", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":search "),
            Span::styled("t", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":tags "),
            Span::styled("f", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":filter "),
            Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":clear "),
            Span::styled("l", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":loop "),
            Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":archive "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":new "),
            Span::styled("g", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":scan "),
            Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":config "),
            Span::styled("s", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cycle "),
            Span::styled("p", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":priority "),
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":refresh"),
        ],
        AppMode::EditingTask { editing_value, .. } => {
            if editing_value.is_some() {
                vec![
                    Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":save "),
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":cancel"),
                ]
            } else {
                vec![
                    Span::styled("Enter/Space", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":edit "),
                    Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":nav "),
                    Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":clear "),
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(":close"),
                ]
            }
        }
        AppMode::CreatingTask(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":create "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::CreatingTaskDescription(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":build "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::Searching(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":search "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::FilteringTags(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":apply "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::FilteringScopes(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":apply "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::EditingConfig { .. } => vec![
            Span::styled("Enter/Space", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":edit "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":nav "),
            Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":clear "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":close"),
        ],
        AppMode::Help => vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":close "),
            Span::styled("?/h", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":close"),
        ],
        AppMode::CommandPalette { .. } => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":run "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":select "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::Scanning(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":scan "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::ConfirmDelete => vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":yes "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":no "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::ConfirmArchive => vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":yes "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":no "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::ConfirmQuit => vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":quit "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":stay "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::ConfirmRevert { .. } => vec![
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":select "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":confirm "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":keep"),
        ],
        AppMode::Executing { .. } => vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":return "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":scroll "),
            Span::styled("PgUp/PgDn", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":page "),
            Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":autoscroll "),
            Span::styled("l", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":stop loop"),
        ],
    };

    if app.save_error.is_some() {
        help_text.push(Span::raw(" "));
        help_text.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
        help_text.push(Span::raw(" "));
        help_text.push(Span::styled(
            "SAVE ERROR",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(msg) = app.status_message.as_deref() {
        help_text.push(Span::raw(" "));
        help_text.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
        help_text.push(Span::raw(" "));
        help_text.push(Span::styled(
            msg.to_string(),
            Style::default().fg(Color::Yellow),
        ));
    }

    help_text
}
