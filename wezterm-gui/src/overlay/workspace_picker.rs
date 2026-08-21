//! Picks where a new workspace runs: this machine, or one of the remote hosts.

use crate::remote::PickerEntry;
use crate::scripting::guiwin::GuiWin;
use crate::termwindow::TermWindowNotif;
use mux::termwiztermtab::TermWizTerminal;
use termwiz::cell::{unicode_column_width, AttributeChange};
use termwiz::color::ColorAttribute;
use termwiz::input::{InputEvent, KeyCode, KeyEvent, MouseButtons, MouseEvent};
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::Terminal;
use window::WindowOps;

pub fn show_workspace_picker_overlay(
    mut term: TermWizTerminal,
    entries: Vec<PickerEntry>,
    window: GuiWin,
) -> anyhow::Result<()> {
    let chosen = run_picker(&mut term, &entries)?;

    let Some(index) = chosen else {
        return Ok(());
    };
    let target = entries[index].target.clone();

    promise::spawn::spawn_into_main_thread(async move {
        window
            .window
            .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                term_window.spawn_workspace_target(target);
            })));
        anyhow::Result::<()>::Ok(())
    })
    .detach();

    Ok(())
}

fn run_picker(
    term: &mut TermWizTerminal,
    entries: &[PickerEntry],
) -> anyhow::Result<Option<usize>> {
    term.set_raw_mode()?;
    term.no_grab_mouse_in_raw_mode();

    let mut filter = String::new();
    let mut selected = 0usize;

    loop {
        let matching = matching_indices(entries, &filter);
        selected = selected.min(matching.len().saturating_sub(1));

        let size = term.get_screen_size()?;
        let first_row = 2;
        let visible = size.rows.saturating_sub(first_row + 2).max(1);
        let scroll = selected.saturating_sub(visible.saturating_sub(1));

        let mut changes = vec![
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
            AttributeChange::Reverse(true).into(),
            Change::Text(format!(" New workspace {:width$}", "", width = size.cols.saturating_sub(15))),
            AttributeChange::Reverse(false).into(),
        ];

        for (row, index) in matching.iter().skip(scroll).take(visible).enumerate() {
            let entry = &entries[*index];
            let is_selected = scroll + row == selected;

            changes.push(Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(first_row + row),
            });
            if is_selected {
                changes.push(AttributeChange::Reverse(true).into());
            }

            let detail_width = unicode_column_width(&entry.detail, None);
            let label_width = size
                .cols
                .saturating_sub(detail_width + 5)
                .max(1);
            changes.push(Change::Text(format!(
                " {} {:<label_width$} {} ",
                if is_selected { ">" } else { " " },
                truncate(&entry.label, label_width),
                entry.detail,
            )));

            if is_selected {
                changes.push(AttributeChange::Reverse(false).into());
            }
        }

        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(size.rows.saturating_sub(1)),
        });
        changes.push(Change::Text(if filter.is_empty() {
            "type to filter · ↑↓ select · enter open · esc cancel".to_string()
        } else {
            format!("filter: {filter}")
        }));
        term.render(&changes)?;

        let Some(event) = term.poll_input(None)? else {
            return Ok(None);
        };

        match event {
            InputEvent::Key(KeyEvent { key, .. }) => match key {
                KeyCode::Escape => return Ok(None),
                KeyCode::Enter => {
                    return Ok(matching.get(selected).copied());
                }
                KeyCode::UpArrow | KeyCode::Char('\u{10}') => {
                    selected = selected.saturating_sub(1);
                }
                KeyCode::DownArrow | KeyCode::Char('\u{e}') => {
                    if selected + 1 < matching.len() {
                        selected += 1;
                    }
                }
                KeyCode::Backspace => {
                    filter.pop();
                    selected = 0;
                }
                KeyCode::Char(c) if !c.is_control() => {
                    filter.push(c);
                    selected = 0;
                }
                _ => {}
            },
            InputEvent::Mouse(MouseEvent {
                y, mouse_buttons, ..
            }) => {
                let row = (y as usize).saturating_sub(first_row);
                if row < matching.len().saturating_sub(scroll) {
                    selected = scroll + row;
                    if mouse_buttons.contains(MouseButtons::LEFT) {
                        return Ok(matching.get(selected).copied());
                    }
                }
            }
            InputEvent::Resized { .. } => {}
            _ => {}
        }
    }
}

fn matching_indices(entries: &[PickerEntry], filter: &str) -> Vec<usize> {
    if filter.is_empty() {
        return (0..entries.len()).collect();
    }
    let needle = filter.to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            entry.label.to_lowercase().contains(&needle)
                || entry.detail.to_lowercase().contains(&needle)
        })
        .map(|(index, _)| index)
        .collect()
}

fn truncate(text: &str, width: usize) -> String {
    if unicode_column_width(text, None) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        if unicode_column_width(&out, None) + 1 >= width {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

