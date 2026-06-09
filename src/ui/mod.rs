mod overlays;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui_image::StatefulImage;

use crate::api::models::Card;
use crate::app::{App, DeckRow, Focus, Mode};
use crate::deck::validate;

const ACCENT: Color = Color::Cyan;

pub fn draw(f: &mut Frame, app: &mut App) {
    let [main, status] =
        *Layout::vertical([Constraint::Min(5), Constraint::Length(1)]).split(f.area())
    else {
        return;
    };

    let columns = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(33),
        Constraint::Percentage(33),
    ])
    .split(main);

    let left = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(columns[0]);

    draw_search_box(f, app, left[0]);
    draw_results(f, app, left[1]);
    draw_deck(f, app, columns[1]);
    draw_detail(f, app, columns[2]);
    draw_status_bar(f, app, status);

    match app.mode.clone() {
        Mode::Help => overlays::draw_help(f),
        Mode::Stats => overlays::draw_stats(f, app),
        Mode::DeckPicker => overlays::draw_deck_picker(f, app),
        Mode::CategoryInput { target } => overlays::draw_input(
            f,
            &format!("Category for {target} (empty = automatic)"),
            &app.input_buffer,
        ),
        Mode::RenameInput => overlays::draw_input(f, "Rename deck", &app.input_buffer),
        Mode::NewDeckInput => overlays::draw_input(f, "New deck name", &app.input_buffer),
        _ => {}
    }
}

fn focus_style(focused: bool) -> Style {
    if focused {
        Style::new().fg(ACCENT)
    } else {
        Style::new().fg(Color::DarkGray)
    }
}

fn draw_search_box(f: &mut Frame, app: &App, area: Rect) {
    let editing = app.mode == Mode::SearchInput;
    let block = Block::bordered()
        .title(" Search [/] ")
        .border_style(focus_style(editing));
    let text = if editing {
        format!("{}\u{2588}", app.search_input) // block cursor
    } else if app.search_input.is_empty() {
        "name, t:type, c:rg, id:wub, o:\"text\"".to_string()
    } else {
        app.search_input.clone()
    };
    let style = if !editing && app.search_input.is_empty() {
        Style::new().fg(Color::DarkGray).italic()
    } else {
        Style::new()
    };
    f.render_widget(Paragraph::new(text).style(style).block(block), area);
}

fn mana_cost_spans(cost: &str) -> Vec<Span<'static>> {
    cost.split(['{', '}'])
        .filter(|s| !s.is_empty())
        .map(|sym| {
            let style = match sym {
                "W" => Style::new().fg(Color::Yellow),
                "U" => Style::new().fg(Color::Blue),
                "B" => Style::new().fg(Color::Magenta),
                "R" => Style::new().fg(Color::Red),
                "G" => Style::new().fg(Color::Green),
                _ => Style::new().fg(Color::Gray),
            };
            Span::styled(format!("({sym})"), style)
        })
        .collect()
}

fn draw_results(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Results && app.mode == Mode::Normal;
    let title = if app.searching {
        " Results — searching... ".to_string()
    } else if app.results.is_empty() {
        " Results ".to_string()
    } else {
        let pages = if app.has_next_page() || app.page > 1 {
            format!(" p{} [n/p] ", app.page)
        } else {
            String::new()
        };
        format!(" Results ({}){}", app.results.len(), pages)
    };
    let block = Block::bordered()
        .title(title)
        .border_style(focus_style(focused));

    let items: Vec<ListItem> = app
        .results
        .iter()
        .map(|card| {
            let mut spans = vec![Span::styled(
                card.name.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            )];
            if let Some(cost) = &card.mana_cost {
                spans.push(Span::raw(" "));
                spans.extend(mana_cost_spans(cost));
            }
            let mut lines = vec![Line::from(spans)];
            lines.push(Line::from(Span::styled(
                format!("  {}", card.type_line.as_deref().unwrap_or("")),
                Style::new().fg(Color::DarkGray),
            )));
            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Rgb(40, 50, 60)))
        .highlight_symbol("▸");
    let mut state = ListState::default().with_selected(if app.results.is_empty() {
        None
    } else {
        Some(app.results_selected)
    });
    f.render_stateful_widget(list, area, &mut state);
}

fn identity_dots(card: &Card) -> Vec<Span<'static>> {
    card.identity()
        .iter()
        .map(|c| {
            let style = match c.as_str() {
                "W" => Style::new().fg(Color::Yellow),
                "U" => Style::new().fg(Color::Blue),
                "B" => Style::new().fg(Color::Magenta),
                "R" => Style::new().fg(Color::Red),
                "G" => Style::new().fg(Color::Green),
                _ => Style::new().fg(Color::Gray),
            };
            Span::styled("●", style)
        })
        .collect()
}

fn draw_deck(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Deck && app.mode == Mode::Normal;
    let issues = validate::validate(&app.deck);
    let badge = if issues.is_empty() {
        Span::styled(" ✓ legal ", Style::new().fg(Color::Green))
    } else {
        Span::styled(
            format!(" ✗ {} issues ", issues.len()),
            Style::new().fg(Color::Red),
        )
    };
    let dirty = if app.dirty { "*" } else { "" };
    let title = Line::from(vec![
        Span::raw(format!(
            " {}{} — {}/100 ",
            app.deck.name,
            dirty,
            app.deck.card_count()
        )),
        badge,
    ]);
    let block = Block::bordered()
        .title(title)
        .border_style(focus_style(focused));

    let rows = app.deck_rows();
    app.deck_selected = app.deck_selected.min(rows.len().saturating_sub(1));

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            DeckRow::CommanderHeader => ListItem::new(Line::from(Span::styled(
                "Commander",
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ))),
            DeckRow::CommanderCard => {
                let cmdr = app.deck.commander.as_ref();
                let name = cmdr.map(|c| c.name.clone()).unwrap_or_default();
                let mut spans = vec![Span::raw("  ♔ "), Span::styled(name, Style::new().bold())];
                if let Some(c) = cmdr {
                    spans.push(Span::raw(" "));
                    spans.extend(identity_dots(c));
                }
                ListItem::new(Line::from(spans))
            }
            DeckRow::CommanderEmpty => ListItem::new(Line::from(Span::styled(
                "  (none — press C on a search result)",
                Style::new().fg(Color::DarkGray).italic(),
            ))),
            DeckRow::CategoryHeader { name, count } => {
                let arrow = if app.collapsed.contains(name) { "▸" } else { "▾" };
                ListItem::new(Line::from(Span::styled(
                    format!("{arrow} {name} ({count})"),
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                )))
            }
            DeckRow::Entry { name } => {
                let entry = app.deck.entries.iter().find(|e| &e.card.name == name);
                let qty = entry.map(|e| e.quantity).unwrap_or(1);
                let mut spans = vec![Span::raw(format!("  {qty} "))];
                spans.push(Span::raw(name.clone()));
                if let Some(e) = entry
                    && let Some(cost) = &e.card.mana_cost {
                        spans.push(Span::raw(" "));
                        spans.extend(mana_cost_spans(cost));
                    }
                ListItem::new(Line::from(spans))
            }
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Rgb(40, 50, 60)));
    let mut state = ListState::default().with_selected(if focused && !rows.is_empty() {
        Some(app.deck_selected)
    } else {
        None
    });
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let card = app.detail_card().cloned();
    let block = Block::bordered()
        .title(" Card ")
        .border_style(Style::new().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(card) = card else {
        f.render_widget(
            Paragraph::new("Search with / and browse results to preview cards")
                .style(Style::new().fg(Color::DarkGray).italic())
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    };

    let image_height = (inner.height as f32 * 0.55) as u16;
    let [image_area, info_area] =
        *Layout::vertical([Constraint::Length(image_height), Constraint::Min(3)]).split(inner)
    else {
        return;
    };

    if app.image_for.is_some() {
        f.render_stateful_widget(StatefulImage::default(), image_area, &mut app.image_state);
    } else {
        let msg = if card.image_url.is_some() {
            "loading image..."
        } else {
            "(no image available)"
        };
        f.render_widget(
            Paragraph::new(msg)
                .style(Style::new().fg(Color::DarkGray).italic())
                .centered(),
            image_area,
        );
    }

    f.render_widget(card_info(&card), info_area);
}

fn card_info(card: &Card) -> Paragraph<'static> {
    let mut lines: Vec<Line> = Vec::new();

    let mut title = vec![Span::styled(
        card.name.clone(),
        Style::new().add_modifier(Modifier::BOLD),
    )];
    if let Some(cost) = &card.mana_cost {
        title.push(Span::raw("  "));
        title.extend(mana_cost_spans(cost));
    }
    lines.push(Line::from(title));

    let mut type_line = card.type_line.clone().unwrap_or_default();
    if let (Some(p), Some(t)) = (&card.power, &card.toughness) {
        type_line.push_str(&format!("  [{p}/{t}]"));
    }
    if let Some(l) = &card.loyalty {
        type_line.push_str(&format!("  [{l}]"));
    }
    lines.push(Line::from(Span::styled(
        type_line,
        Style::new().fg(ACCENT),
    )));

    let set = format!(
        "{} · {}",
        card.set_name.as_deref().unwrap_or("?"),
        card.rarity.as_deref().unwrap_or("?")
    );
    lines.push(Line::from(Span::styled(set, Style::new().fg(Color::DarkGray))));
    lines.push(Line::raw(""));

    if let Some(text) = &card.text {
        for l in text.lines() {
            lines.push(Line::raw(l.to_string()));
        }
    }
    if let Some(flavor) = &card.flavor {
        lines.push(Line::raw(""));
        for l in flavor.lines() {
            lines.push(Line::from(Span::styled(
                l.to_string(),
                Style::new().fg(Color::DarkGray).italic(),
            )));
        }
    }
    let legal = if card.is_legal_in_commander() {
        Span::styled("Legal in Commander", Style::new().fg(Color::Green))
    } else {
        Span::styled("NOT legal in Commander", Style::new().fg(Color::Red))
    };
    lines.push(Line::raw(""));
    lines.push(Line::from(legal));

    Paragraph::new(lines).wrap(Wrap { trim: false })
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let content = match &app.status {
        Some((msg, is_err)) => {
            let style = if *is_err {
                Style::new().fg(Color::Red)
            } else {
                Style::new().fg(Color::Green)
            };
            Line::from(Span::styled(format!(" {msg}"), style))
        }
        None => Line::from(Span::styled(
            " [/]search [Tab]switch [a]dd [C]ommander [x]remove [c]ategory [S]tats [w]rite [E]xport [L]oad [D]new [R]ename [?]help [q]uit",
            Style::new().fg(Color::DarkGray),
        )),
    };
    f.render_widget(Paragraph::new(content), area);
}
