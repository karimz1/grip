use crate::app::*;
use oflh_core::*;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Paragraph, Row as TableRow, Table, TableState, Wrap,
    },
};
use unicode_width::UnicodeWidthChar;
pub const ACCENT: Color = Color::Rgb(167, 139, 250);
pub const MUTED: Color = Color::DarkGray;
pub const LOCK: Color = Color::Rgb(229, 140, 140);
fn accent() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
fn selected() -> Style {
    Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(109, 40, 217))
        .add_modifier(Modifier::BOLD)
}
fn access_style(access: Access) -> Style {
    Style::default().fg(match access {
        Access::Read => Color::Rgb(155, 197, 161),
        Access::Write | Access::ReadWrite => Color::Rgb(217, 183, 125),
        Access::Execute | Access::Mapped => ACCENT,
        Access::Directory | Access::Reference => Color::Rgb(154, 174, 196),
        _ => MUTED,
    })
}
pub fn cpu(process: &Process) -> String {
    process
        .cpu
        .map_or_else(|| "—".into(), |value| format!("{value:.1}%"))
}
pub fn memory(process: &Process) -> String {
    process.memory.map_or_else(
        || "—".into(),
        |value| {
            if value >= 1 << 30 {
                format!("{:.1} GiB", value as f64 / (1u64 << 30) as f64)
            } else {
                format!("{:.1} MiB", value as f64 / (1u64 << 20) as f64)
            }
        },
    )
}
fn text(frame: &mut Frame, area: Rect, value: impl Into<String>, style: Style) {
    frame.render_widget(Paragraph::new(value.into()).style(style), area)
}
fn line_area(area: Rect, y: u16) -> Rect {
    Rect::new(area.x, area.y.saturating_add(y), area.width, 1)
}
fn process_access(process: &Process, usages: &[usize]) -> Access {
    let mut read = false;
    let mut write = false;
    for &i in usages {
        match process.usages[i].access {
            Access::Read => read = true,
            Access::Write => write = true,
            Access::ReadWrite => {
                read = true;
                write = true
            }
            _ => {}
        }
    }
    if read && write {
        Access::ReadWrite
    } else if write {
        Access::Write
    } else if read {
        Access::Read
    } else {
        usages
            .first()
            .map_or(Access::Unknown, |&i| process.usages[i].access)
    }
}
fn search(frame: &mut Frame, area: Rect, app: &App, detail: bool) {
    let value = if detail {
        &app.detail_query
    } else {
        &app.query
    };
    let label = if value.is_empty() {
        if detail {
            "/ Search files, DLLs, paths… · * wildcard"
        } else {
            "/ Search PID, process, path… · * wildcard"
        }
    } else {
        value
    };
    let label = if value.is_empty() {
        label.to_owned()
    } else {
        format!("/ {label}")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if app.editing { ACCENT } else { MUTED }));
    let inner = block.inner(area);
    frame.render_widget(
        Paragraph::new(label)
            .style(Style::default().fg(if app.editing { Color::White } else { MUTED }))
            .block(block),
        area,
    );
    if app.editing && inner.width > 2 {
        let position = app.input_cursor.min(value.len());
        let width = value[..position]
            .chars()
            .map(|c| c.width().unwrap_or(0))
            .sum::<usize>();
        frame.set_cursor_position((
            inner.x + (width + 2).min(inner.width as usize - 1) as u16,
            inner.y,
        ));
    }
}
/// Render distinct action colors with a pointer on the focused choice.
fn confirmation_buttons(app: &App) -> Line<'static> {
    let cancel_color = Color::Rgb(155, 197, 161);
    let terminate_color = LOCK;
    let button = |label: &str, focused: bool, color: Color| {
        let style = if focused {
            Style::default()
                .fg(Color::Rgb(24, 24, 32))
                .bg(color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        Span::styled(
            format!("{} {label} ", if focused { "▶" } else { " " }),
            style,
        )
    };
    Line::from(vec![
        button("Cancel", !app.confirm, cancel_color),
        Span::raw("   "),
        button(
            if app.force { "Force kill" } else { "Terminate" },
            app.confirm,
            terminate_color,
        ),
    ])
}

fn footer_lines(width: u16, app: &App) -> Vec<Line<'static>> {
    let (keys, status) = match app.screen {
        Screen::Confirm => ("Tab / ←→ choose · Enter confirm · Esc cancel", ""),
        Screen::Details => (
            "/ search · l locks only · ↑↓ select · ←→ path · r refresh · a auto · k stop · x force · R GitHub · D Donate · Esc back",
            "",
        ),
        Screen::Help => ("↑↓ scroll · R GitHub · D Donate · Esc back", ""),
        Screen::Main if app.tree.is_some() => (
            "↑↓ process · k stop target · x force kill target · Tab/← back",
            "",
        ),
        _ => (
            "1/2 tabs · / search · ↑↓ move · Enter inspect · Space select · Ctrl+A all · Tab/→ tree · i panel · m/c RAM/CPU · a auto · r refresh · k stop · x force · ? help · R GitHub · D Donate · q quit",
            "Enter inspect · Space select · m RAM / c CPU / n name / p PID",
        ),
    };
    let keys = if width < 60 {
        match app.screen {
            Screen::Main if app.tree.is_none() => {
                "1/2 tabs · / search · Enter inspect · k stop · x force · ? help · R GitHub · D Donate · q quit"
            }
            Screen::Details => {
                "↑↓ select · / search · l locks · r refresh · R GitHub · D Donate · Esc back"
            }
            _ => keys,
        }
    } else {
        keys
    };
    let mut lines = Vec::new();
    if !app.status.is_empty() {
        lines.push(Line::styled(
            safe(&app.status),
            Style::default().fg(if app.error { LOCK } else { MUTED }),
        ))
    } else if !status.is_empty() {
        lines.push(Line::styled(status, Style::default().fg(MUTED)))
    }
    if !app.snapshot.warnings.is_empty() && app.screen != Screen::Help {
        lines.push(Line::styled(
            "Results may be incomplete · ? details",
            Style::default().fg(MUTED),
        ))
    }
    lines.push(Line::styled(
        "─".repeat(width as usize),
        Style::default().fg(MUTED),
    ));
    if app.screen == Screen::Confirm {
        lines.push(confirmation_buttons(app));
    }
    // Wrap whole shortcut phrases to retain meaning at narrow widths.
    let mut row = String::new();
    for hint in keys.split(" · ") {
        let sep = if row.is_empty() { "" } else { " · " };
        if unicode_width::UnicodeWidthStr::width(format!("{row}{sep}{hint}").as_str())
            > width as usize
            && !row.is_empty()
        {
            lines.push(Line::styled(std::mem::take(&mut row), accent()))
        }
        if !row.is_empty() {
            row.push_str(" · ")
        }
        row.push_str(hint)
    }
    if !row.is_empty() {
        lines.push(Line::styled(row, accent()))
    }
    if app.editing {
        lines = vec![Line::styled(
            "Enter apply · Esc cancel · ↑↓ browse",
            accent(),
        )]
    }
    lines
}
fn footer(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Paragraph::new(footer_lines(area.width, app)), area);
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let full = frame.area();
    app.width = full.width;
    app.height = full.height;
    let margin = if full.width >= 10 { 2 } else { 0 };
    let area = Rect::new(
        full.x + margin,
        full.y,
        full.width.saturating_sub(margin * 2),
        full.height,
    );
    if area.width == 0 || area.height == 0 {
        return;
    }
    if area.height < 12 || area.width < 20 {
        text(
            frame,
            area,
            format!(
                "oflh {}\n{}\nResize for full view · Esc back · q quit",
                safe(&app.target.path.to_string_lossy()),
                if app.screen == Screen::Confirm {
                    format!("{} targets. Enlarge to review.", app.pending.len())
                } else {
                    app.current().map_or_else(String::new, |process| {
                        format!("{} {}", process.identity.pid, safe(&process.name))
                    })
                }
            ),
            accent(),
        );
        return;
    }
    match app.screen {
        Screen::Main => main_view(frame, area, app),
        Screen::Details => details(frame, area, app),
        Screen::Help | Screen::Confirm => dialog(frame, area, app),
    }
}
fn main_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let tabs = vec![
        Span::styled("oflh  ", accent()),
        Span::styled(
            if area.width < 48 {
                " 1 Proc "
            } else {
                " 1 Processes "
            },
            if !app.locked {
                selected()
            } else {
                Style::default()
                    .fg(Color::Rgb(184, 184, 204))
                    .bg(Color::Rgb(48, 48, 64))
            },
        ),
        Span::styled(
            if area.width < 48 {
                " 2 Locks "
            } else {
                " 2 Locked files "
            },
            if app.locked {
                selected()
            } else {
                Style::default()
                    .fg(Color::Rgb(184, 184, 204))
                    .bg(Color::Rgb(48, 48, 64))
            },
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(tabs)), line_area(area, 0));
    if area.width > 60 {
        text(
            frame,
            Rect::new(
                area.right().saturating_sub(app.version.len() as u16),
                area.y,
                app.version.len() as u16,
                1,
            ),
            safe(&app.version),
            Style::default().fg(MUTED),
        );
    }
    let activity = if app.stopping {
        "requesting termination…".to_owned()
    } else if app.scanning {
        format!("{} scanning", ["◐", "◓", "◑", "◒"][app.pulse % 4])
    } else if app.auto {
        "LIVE · every 5s".into()
    } else {
        "MANUAL · r refresh".into()
    };
    text(
        frame,
        line_area(area, 1),
        format!(
            "{}  ·  {activity}",
            safe(&app.target.path.to_string_lossy())
        ),
        Style::default().fg(MUTED),
    );
    text(
        frame,
        line_area(area, 2),
        if app.editing {
            "Search · Enter apply · Esc cancel"
        } else if app.tree.is_some() {
            "Tree · ↑↓ choose process · k stop / x force kill · Tab/← back"
        } else {
            "Navigation · / search · Tab/→ tree"
        },
        Style::default().fg(MUTED),
    );
    search(
        frame,
        Rect::new(area.x, area.y + 3, area.width, 3),
        app,
        false,
    );
    let summary = if app.locked {
        format!(
            "{} locked files · {} lock entries",
            app.rows
                .iter()
                .map(|row| &app.snapshot.processes[row.process].usages[row.usages[0]].path)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            app.rows.len()
        )
    } else {
        format!(
            "{} of {} processes",
            app.rows.len(),
            app.snapshot.processes.len()
        )
    };
    text(
        frame,
        line_area(area, 6),
        format!(
            "{summary}{} · sort: {}",
            if app.selected.is_empty() {
                String::new()
            } else {
                format!(" · {} selected", app.selected.len())
            },
            match app.sort {
                Sort::Name => "name",
                Sort::Pid => "pid",
                Sort::Memory => "RAM",
                Sort::Cpu => "CPU",
                Sort::Relevance =>
                    if app.query.is_empty() {
                        "pid"
                    } else {
                        "match"
                    },
            }
        ),
        accent(),
    );
    let footer_h = (footer_lines(area.width, app).len() as u16).min(area.height.saturating_sub(9));
    let body_h = area.height.saturating_sub(7 + footer_h);
    let body = Rect::new(area.x, area.y + 7, area.width, body_h);
    app.page = body_h.saturating_sub(1).max(1) as usize;
    let show =
        app.tree.is_some() || (!app.hide_inspector && area.width >= 116 && area.height >= 22);
    let panel_w = if show {
        if app.tree.is_some() && area.width < 116 {
            area.width
        } else {
            46.min(area.width / 3)
        }
    } else {
        0
    };
    let table_w = if panel_w == area.width {
        0
    } else {
        area.width
            .saturating_sub(if panel_w > 0 { panel_w + 3 } else { 0 })
    };
    if table_w > 0 {
        let table_area = Rect::new(body.x, body.y, table_w, body.height);
        render_main_table(frame, table_area, app)
    }
    if panel_w > 0 {
        inspector(
            frame,
            Rect::new(body.right() - panel_w, body.y, panel_w, body.height),
            app,
        )
    }
    footer(
        frame,
        Rect::new(area.x, area.bottom() - footer_h, area.width, footer_h),
        app,
    );
}
fn render_main_table(frame: &mut Frame, area: Rect, app: &App) {
    if app.locked {
        return locked_table(frame, area, app);
    }
    if app.rows.is_empty() {
        text(
            frame,
            area,
            if app.scanning && app.snapshot.processes.is_empty() {
                "Discovering processes…\nYou can keep navigating while the scan runs."
            } else if !app.query.is_empty() {
                "No processes match your filter.\nPress / to edit it or Esc to clear."
            } else {
                "No visible processes are using this path.\nPress r to scan again. Permissions may hide usage."
            },
            Style::default().fg(MUTED),
        );
        return;
    }
    let count = area.height.saturating_sub(1).max(1) as usize;
    let start = app
        .cursor
        .saturating_sub(count - 1)
        .min(app.rows.len().saturating_sub(count));
    let wide = area.width >= 94;
    let medium = area.width >= 62;
    let widths = if wide {
        vec![
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Length(18),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(11),
            Constraint::Length(12),
            Constraint::Min(8),
        ]
    } else if medium {
        vec![
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Length(19),
            Constraint::Length(12),
            Constraint::Min(4),
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Percentage(40),
            Constraint::Min(4),
        ]
    };
    let headers = if wide {
        vec![
            "",
            "PID",
            "PROCESS",
            "USER",
            "CPU%",
            "RAM",
            "ACCESS",
            "MATCHED PATH",
        ]
    } else if medium {
        vec!["", "PID", "PROCESS", "ACCESS", "MATCHED PATH"]
    } else {
        vec!["", "PID", "PROCESS", "PATH"]
    };
    let rows = app.rows.iter().skip(start).take(count).map(|row| {
        let process = &app.snapshot.processes[row.process];
        let usage = &process.usages[row.usages[0]];
        let mark = if app.selected.contains(&process.identity) {
            "●"
        } else {
            " "
        };
        let path = if app.target.directory {
            usage
                .path
                .strip_prefix(&app.target.path)
                .unwrap_or(&usage.path)
        } else {
            &usage.path
        };
        let path = format!(
            "{}{}{}",
            safe(&path.to_string_lossy()),
            if usage.deleted { " (deleted)" } else { "" },
            if row.usages.len() > 1 {
                format!(" +{}", row.usages.len() - 1)
            } else {
                String::new()
            }
        );
        let access = process_access(process, &row.usages);
        let mut cells = vec![
            Cell::from(mark),
            Cell::from(process.identity.pid.to_string()),
            Cell::from(safe(&process.name)),
        ];
        if wide {
            cells.extend([
                Cell::from(safe(&process.user)),
                Cell::from(cpu(process)),
                Cell::from(memory(process)),
            ]);
        }
        if medium {
            cells.push(
                Cell::from(if app.locked { "locked" } else { access.label() }).style(
                    if app.locked {
                        Style::default().fg(LOCK)
                    } else {
                        access_style(access)
                    },
                ),
            )
        }
        cells.push(Cell::from(path));
        TableRow::new(cells)
    });
    let table = Table::new(rows, widths)
        .header(TableRow::new(headers).style(Style::default().fg(MUTED)))
        .row_highlight_style(selected())
        .column_spacing(0);
    let mut state = TableState::default().with_selected(Some(app.cursor - start));
    frame.render_stateful_widget(table, area, &mut state);
}
fn inspector(frame: &mut Frame, area: Rect, app: &App) {
    let captured;
    let process = if let Some(tree) = &app.tree {
        let Some(child) = tree.nodes.last() else {
            return;
        };
        captured = Process {
            identity: child.identity,
            name: child.name.clone(),
            ..Process::default()
        };
        app.snapshot
            .processes
            .iter()
            .find(|process| process.identity == child.identity)
            .unwrap_or(&captured)
    } else {
        let Some(process) = app.current() else { return };
        process
    };
    let mut lines = vec![
        Line::styled(
            format!("{} · PID {}", safe(&process.name), process.identity.pid),
            accent(),
        ),
        Line::styled("─".repeat(area.width as usize), Style::default().fg(MUTED)),
        Line::raw(format!("CPU {}   RAM {}", cpu(process), memory(process))),
        Line::styled(
            "CPU = share of machine · RAM = RSS",
            Style::default().fg(MUTED),
        ),
        Line::raw(""),
        Line::styled(
            if app.tree.is_some() {
                "ANCESTRY · focused"
            } else {
                "ANCESTRY"
            },
            accent(),
        ),
    ];
    let owned;
    let nodes = if let Some(tree) = &app.tree {
        &tree.nodes
    } else {
        owned = process
            .ancestors
            .iter()
            .rev()
            .map(|access| ActionTarget {
                identity: access.identity,
                name: access.name.clone(),
            })
            .chain(std::iter::once(ActionTarget {
                identity: process.identity,
                name: process.name.clone(),
            }))
            .collect::<Vec<_>>();
        &owned
    };
    let count = (area.height as usize)
        .saturating_sub(lines.len() + 5)
        .max(2);
    let cursor = app
        .tree
        .as_ref()
        .map_or(nodes.len().saturating_sub(1), |t| t.cursor);
    let start = cursor.saturating_sub(count.saturating_sub(2));
    let mut indices: Vec<_> = (start..nodes.len()).take(count.saturating_sub(1)).collect();
    if !indices.contains(&nodes.len().saturating_sub(1)) {
        indices.push(nodes.len().saturating_sub(1))
    }
    for i in indices {
        let node = &nodes[i];
        lines.push(Line::styled(
            format!(
                "{}└─ {} ({})",
                "  ".repeat(i.min(8)),
                safe(&node.name),
                node.identity.pid
            ),
            if app.tree.is_some() && i == cursor {
                selected()
            } else if i + 1 == nodes.len() {
                accent()
            } else {
                Style::default().fg(MUTED)
            },
        ))
    }
    if let Some(tree) = &app.tree {
        let node = &tree.nodes[tree.cursor];
        lines.extend([
            Line::raw(""),
            Line::styled("ACTION TARGET", accent()),
            Line::raw(format!("{} · PID {}", safe(&node.name), node.identity.pid)),
        ]);
    }
    lines.extend([
        Line::raw(""),
        Line::styled("EXECUTABLE", accent()),
        Line::raw(safe(&process.executable.to_string_lossy())),
    ]);
    if let Some(row) = app.rows.get(app.cursor) {
        lines.extend([
            Line::raw(""),
            Line::styled("SELECTED PATH", accent()),
            Line::raw(safe(&process.usages[row.usages[0]].path.to_string_lossy())),
        ]);
    }
    frame.render_widget(Paragraph::new(lines), area);
}
fn details(frame: &mut Frame, area: Rect, app: &mut App) {
    text(
        frame,
        line_area(area, 0),
        "oflh  /  process details",
        accent(),
    );
    let Some(process) = app.detail() else {
        let footer_height = (footer_lines(area.width, app).len() as u16).min(area.height);
        text(
            frame,
            Rect::new(area.x, area.y + 2, area.width, 3),
            "Process exited or PID was reused. Press Esc to return.",
            Style::default().fg(LOCK),
        );
        footer(
            frame,
            Rect::new(
                area.x,
                area.bottom() - footer_height,
                area.width,
                footer_height,
            ),
            app,
        );
        return;
    };
    let header = vec![
        Line::styled(
            format!(
                "{}   PID {} · {}",
                safe(&process.name),
                process.identity.pid,
                safe(&process.user)
            ),
            accent(),
        ),
        Line::raw(format!(
            "EXE {}",
            safe(&process.executable.to_string_lossy())
        )),
        Line::raw(format!("CWD {}", safe(&process.cwd.to_string_lossy()))),
        Line::raw(format!(
            "PARENT {}",
            process.ancestors.first().map_or_else(
                || "unavailable".into(),
                |access| format!("{} ({})", safe(&access.name), access.identity.pid)
            )
        )),
        Line::raw(format!(
            "CPU {} machine · RAM {} RSS",
            cpu(process),
            memory(process)
        )),
    ];
    let compact = area.height < 24;
    let head_height = if compact { 3 } else { 6 };
    if compact {
        let lines = vec![
            header[0].clone(),
            Line::raw(format!(
                "CPU {} · RAM {} · PARENT {}",
                cpu(process),
                memory(process),
                process.parent
            )),
        ];
        frame.render_widget(
            Paragraph::new(lines),
            Rect::new(area.x, area.y + 1, area.width, 2),
        );
    } else {
        frame.render_widget(
            Paragraph::new(header),
            Rect::new(area.x, area.y + 1, area.width, 5),
        );
    }
    search(
        frame,
        Rect::new(area.x, area.y + head_height, area.width, 3),
        app,
        true,
    );
    let locked = app
        .usage_rows
        .iter()
        .filter_map(|&i| {
            process.usages[i]
                .lock
                .as_ref()
                .map(|_| &process.usages[i].path)
        })
        .collect::<std::collections::HashSet<_>>()
        .len();
    text(
        frame,
        line_area(area, head_height + 3),
        format!(
            "{} · {} of {} usages · {locked} locked files   {} / {}",
            if app.detail_locks {
                "LOCKS ONLY"
            } else {
                "ALL USAGES"
            },
            app.usage_rows.len(),
            process.usages.len(),
            if app.usage_rows.is_empty() {
                0
            } else {
                app.usage_cursor + 1
            },
            app.usage_rows.len()
        ),
        accent(),
    );
    let current = app
        .usage_rows
        .get(app.usage_cursor)
        .and_then(|&i| process.usages.get(i));
    text(
        frame,
        line_area(area, head_height + 4),
        format!(
            "FILE {}",
            current.map_or_else(String::new, |usage| safe(
                &usage.path.file_name().unwrap_or_default().to_string_lossy()
            ))
        ),
        Style::default().fg(MUTED),
    );
    let foot_h = (footer_lines(area.width, app).len() as u16).min(area.height.saturating_sub(9));
    let body_h = area.height.saturating_sub(head_height + 8 + foot_h);
    let body = Rect::new(area.x, area.y + head_height + 5, area.width, body_h);
    if app.usage_rows.is_empty() {
        text(
            frame,
            body,
            "No usage entries match this filter.",
            Style::default().fg(MUTED),
        )
    } else {
        let count = body.height.saturating_sub(2).max(1) as usize;
        let start = app
            .usage_cursor
            .saturating_sub(count - 1)
            .min(app.usage_rows.len().saturating_sub(count));
        let wide = area.width >= 65;
        let widths = if wide {
            vec![
                Constraint::Percentage(35),
                Constraint::Length(14),
                Constraint::Length(13),
                Constraint::Min(4),
            ]
        } else {
            vec![Constraint::Min(8), Constraint::Length(11)]
        };
        let rows =
            app.usage_rows.iter().skip(start).take(count).map(|&i| {
                let usage = &process.usages[i];
                let mut cells = vec![Cell::from(safe(
                    &usage.path.file_name().unwrap_or_default().to_string_lossy(),
                ))];
                if wide {
                    cells.push(Cell::from(usage.relation.label()).style(
                        Style::default().fg(if usage.lock.is_some() { LOCK } else { ACCENT }),
                    ));
                    cells.push(Cell::from(usage.access.label()).style(access_style(usage.access)));
                    cells.push(Cell::from(safe(
                        &usage
                            .path
                            .parent()
                            .unwrap_or(std::path::Path::new(""))
                            .to_string_lossy(),
                    )))
                } else {
                    cells.push(Cell::from(usage.relation.label()))
                }
                TableRow::new(cells)
            });
        let headers = if wide {
            vec!["FILE", "RELATION", "ACCESS", "DIRECTORY"]
        } else {
            vec!["FILE", "RELATION"]
        };
        let table = Table::new(rows, widths)
            .column_spacing(1)
            .header(TableRow::new(headers).style(accent()))
            .row_highlight_style(selected())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(MUTED)),
            );
        let mut state = TableState::default().with_selected(Some(app.usage_cursor - start));
        frame.render_stateful_widget(table, body, &mut state);
    }
    let preview_y = area.bottom().saturating_sub(foot_h + 3);
    if let Some(usage) = current {
        text(
            frame,
            Rect::new(area.x, preview_y, area.width, 1),
            format!(
                "SELECTED PATH · {} · {}",
                usage.relation.label(),
                usage.access.label()
            ),
            access_style(usage.access),
        );
        let full = format!(
            "{}{}",
            safe(&usage.path.to_string_lossy()),
            usage
                .lock
                .as_ref()
                .map_or_else(String::new, |l| format!(" · {}", safe(&l.to_string())))
        );
        let page_width = area.width.max(1) as usize;
        let chars: Vec<_> = full.chars().collect();
        let position = app
            .path_page
            .min(chars.len().saturating_sub(1) / page_width)
            * page_width;
        let preview: String = chars[position..].iter().collect();
        text(
            frame,
            Rect::new(area.x, preview_y + 1, area.width, 1),
            preview,
            Style::default(),
        );
    }
    app.page = body_h.saturating_sub(2).max(1) as usize;
    footer(
        frame,
        Rect::new(area.x, area.bottom() - foot_h, area.width, foot_h),
        app,
    );
}
fn dialog(frame: &mut Frame, area: Rect, app: &mut App) {
    text(
        frame,
        line_area(area, 0),
        "oflh  ·  Open File Lock Handle",
        accent(),
    );
    let mut lines = Vec::new();
    if app.screen == Screen::Confirm {
        lines.push(Line::styled(
            format!(
                "{} {} processes?",
                if app.force { "FORCE KILL" } else { "Terminate" },
                app.pending.len()
            ),
            Style::default().fg(LOCK).add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::raw(if app.force {
            "Immediate termination: no cleanup. Unsaved work may be lost."
        } else {
            "Request a normal shutdown. Unsaved work may be lost."
        }));
        if app.parent_action {
            lines.push(Line::styled(
                "Target: selected ancestor. Its application and children may be affected.",
                Style::default().fg(LOCK),
            ))
        }
        lines.push(Line::raw(""));
        lines.push(Line::raw(
            "Affected processes (including selections hidden by filters):",
        ));
        for process in &app.pending {
            lines.push(Line::raw(format!(
                "  {:<9} {}",
                process.identity.pid,
                safe(&process.name)
            )))
        }
    } else {
        lines.push(Line::styled("LINKS", accent()));
        lines.push(Line::raw(
            "R  GitHub: https://github.com/karimz1/open-file-lock-handle",
        ));
        lines.push(Line::raw("D  Donate: https://buymeacoffee.com/karimz1"));
        lines.push(Line::raw("Press uppercase R or D to open in your browser."));
        lines.push(Line::raw(""));
        if !app.snapshot.warnings.is_empty() {
            lines.push(Line::styled("SCAN DETAILS", accent()));
        }
        for warning in &app.snapshot.warnings {
            lines.push(Line::raw(safe(warning)))
        }
        lines.push(Line::raw(""));
        lines.extend(HELP.lines().map(|s| Line::raw(s.to_owned())));
    }
    let footer_height = (footer_lines(area.width, app).len() as u16).min(area.height);
    let height = area.height.saturating_sub(footer_height + 2);
    app.page = height.max(1) as usize;
    app.scroll = app.scroll.min(lines.len().saturating_sub(1));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll.min(u16::MAX as usize) as u16, 0)),
        Rect::new(area.x, area.y + 2, area.width, height),
    );
    footer(
        frame,
        Rect::new(
            area.x,
            area.bottom() - footer_height,
            area.width,
            footer_height,
        ),
        app,
    );
}
const HELP: &str = "OPEN FILE LOCK HANDLE
1 / 2          Processes / locked files
↑ / ↓, j       Navigate results
PgUp / PgDn    Move one page
Home / End     First / last result
Enter          Inspect matching paths
Space          Select process
Ctrl+A         Select / deselect all visible processes
/              Search PID, name, user, path, access
*              Wildcards: micro*dll, FLEC*.json
               Fragments and CamelCase; spaces combine terms.
Esc            Clear search / back / cancel
r              Refresh; cancels previous scan
a              Toggle five-second auto-refresh
i              Toggle side inspector
Tab / →        Focus ancestry; ↑↓ chooses action target
Tab / ← / Esc  Leave ancestry
l              Details: toggle locks only
m / c          Sort RAM / CPU descending
n / p          Sort name / PID
k / x          Stop / force kill selection or current process
K / X          Selection; otherwise all filtered processes
Tab            Choose Cancel / Terminate
?              Show help
R / D          Open repository / donation page
q / Ctrl+C     Back / quit

Every termination requires confirmation. Cancel is the default.
Hidden selections are included. Process identity is revalidated.
Stopping a parent does not recursively terminate its children.

READING THE EVIDENCE
An open file is not necessarily locked.
locked         Platform lock / sharing-conflict evidence
open           Observed file descriptor
cwd            Current working directory
executable     Process executable
mapped         Mapped file / loaded module
restart manager  Windows resource user, owner unverified
unknown        OS did not expose this information

CPU is a share of total machine capacity, sampled twice.
Permissions, namespaces and races may limit visibility.";

fn locked_table(frame: &mut Frame, area: Rect, app: &App) {
    if app.rows.is_empty() {
        text(
            frame,
            area,
            if app.scanning {
                "Scanning for file locks…"
            } else if !app.query.is_empty() {
                "No locked files match your filter."
            } else {
                "No confirmed locks found in this scan.\nVisibility depends on permissions; press r to refresh."
            },
            Style::default().fg(MUTED),
        );
        return;
    }
    let count = area.height.saturating_sub(1).max(1) as usize;
    let start = app
        .cursor
        .saturating_sub(count - 1)
        .min(app.rows.len().saturating_sub(count));
    let show_name = area.width >= 40;
    let mut widths = vec![Constraint::Length(3), Constraint::Length(8)];
    let mut headers = vec!["", "PID"];
    if show_name {
        widths.push(Constraint::Length(if area.width < 62 { 10 } else { 18 }));
        headers.push("PROCESS");
    }
    widths.push(Constraint::Min(4));
    headers.push("LOCKED FILE");
    let rows = app.rows.iter().skip(start).take(count).map(|row| {
        let process = &app.snapshot.processes[row.process];
        let usage = &process.usages[row.usages[0]];
        let mut cells = vec![
            Cell::from(if app.selected.contains(&process.identity) {
                "●"
            } else {
                " "
            }),
            Cell::from(process.identity.pid.to_string()),
        ];
        if show_name {
            cells.push(Cell::from(safe(&process.name)))
        }
        cells.push(Cell::from(format!(
            "{}{}",
            safe(&usage.path.to_string_lossy()),
            if usage.deleted { " (deleted)" } else { "" }
        )));
        TableRow::new(cells)
    });
    let table = Table::new(rows, widths)
        .column_spacing(0)
        .header(TableRow::new(headers).style(Style::default().fg(MUTED)))
        .row_highlight_style(selected());
    let mut state = TableState::default().with_selected(Some(app.cursor - start));
    frame.render_stateful_widget(table, area, &mut state);
}
