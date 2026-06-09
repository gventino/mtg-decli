use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;
use crate::deck::stats::{self, PIP_LABELS};

const ACCENT: Color = Color::Cyan;

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = *Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(area)
    else {
        return area;
    };
    let [area] = *Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area)
    else {
        return area;
    };
    area
}

pub fn draw_input(f: &mut Frame, title: &str, value: &str) {
    let area = centered(f.area(), 60, 3);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .title(format!(" {title} "))
        .border_style(Style::new().fg(ACCENT));
    f.render_widget(
        Paragraph::new(format!("{value}\u{2588}")).block(block),
        area,
    );
}

pub fn draw_help(f: &mut Frame) {
    let lines: Vec<(&str, &str)> = vec![
        ("/", "edit search (Enter runs it, Esc cancels)"),
        ("", "  filters: t:creature c:rg id:wub o:\"draw a card\""),
        ("", "  r:rare s:CMD f:any (default format: Commander)"),
        ("", "  scryfall source: full Scryfall syntax supported"),
        ("o", "switch card source (Scryfall ⇄ MTG API)"),
        ("Tab", "switch focus between Results and Deck"),
        ("j/k ↑/↓ g/G", "navigate lists"),
        ("n / p", "next / previous results page"),
        ("a / Enter", "add selected result to deck"),
        ("C", "set selected result as commander"),
        ("x / d", "remove selected deck card (or commander)"),
        ("c", "set custom category for selected deck card"),
        ("Space/Enter", "collapse/expand category (on header)"),
        ("S", "deck statistics"),
        ("w", "save deck (JSON)"),
        ("E", "export deck (.txt, Moxfield format)"),
        ("L", "load a saved deck"),
        ("D", "new deck"),
        ("R", "rename deck"),
        ("q / Ctrl-C", "quit (autosaves)"),
    ];
    let height = lines.len() as u16 + 2;
    let area = centered(f.area(), 64, height);
    f.render_widget(Clear, area);
    let text: Vec<Line> = lines
        .into_iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!(" {key:<12}"),
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw(desc),
            ])
        })
        .collect();
    let block = Block::bordered()
        .title(" Help — press Esc to close ")
        .border_style(Style::new().fg(ACCENT));
    f.render_widget(Paragraph::new(text).block(block), area);
}

pub fn draw_stats(f: &mut Frame, app: &App) {
    let s = stats::compute(&app.deck);
    let area = centered(f.area(), 56, 22);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .title(format!(" Stats — {} ", app.deck.name))
        .border_style(Style::new().fg(ACCENT));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Mana curve (nonland)",
        Style::new().bold(),
    )));
    let max = s.curve.iter().copied().max().unwrap_or(0).max(1);
    for (i, count) in s.curve.iter().enumerate() {
        let label = if i == 7 { "7+".to_string() } else { i.to_string() };
        let width = (count * 30).div_ceil(max);
        lines.push(Line::from(vec![
            Span::styled(format!("{label:>3} "), Style::new().fg(Color::DarkGray)),
            Span::styled("█".repeat(width as usize), Style::new().fg(ACCENT)),
            Span::raw(format!(" {count}")),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("Color pips", Style::new().bold())));
    let pip_colors = [
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Red,
        Color::Green,
        Color::Gray,
    ];
    let pip_max = s.color_pips.iter().copied().max().unwrap_or(0).max(1);
    for (i, count) in s.color_pips.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let width = (count * 30).div_ceil(pip_max);
        lines.push(Line::from(vec![
            Span::styled(format!("{:>3} ", PIP_LABELS[i]), Style::new().fg(pip_colors[i])),
            Span::styled("█".repeat(width as usize), Style::new().fg(pip_colors[i])),
            Span::raw(format!(" {count}")),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Cards: ", Style::new().bold()),
        Span::raw(format!(
            "{}/100   lands {}   nonlands {}   avg cmc {:.2}",
            app.deck.card_count(),
            s.lands,
            s.nonlands,
            s.avg_cmc
        )),
    ]));
    let types = s
        .type_counts
        .iter()
        .map(|(n, c)| format!("{n} {c}"))
        .collect::<Vec<_>>()
        .join(" · ");
    lines.push(Line::from(Span::styled(
        types,
        Style::new().fg(Color::DarkGray),
    )));

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

pub fn draw_deck_picker(f: &mut Frame, app: &App) {
    let height = (app.saved_decks.len() as u16 + 2).clamp(3, 20);
    let area = centered(f.area(), 50, height);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Load deck — Enter to load, Esc to close ")
        .border_style(Style::new().fg(ACCENT));

    if app.saved_decks.is_empty() {
        f.render_widget(
            Paragraph::new("No saved decks yet — press w to save one")
                .style(Style::new().fg(Color::DarkGray).italic())
                .block(block),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .saved_decks
        .iter()
        .map(|(name, _)| ListItem::new(Line::raw(name.clone())))
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Rgb(40, 50, 60)))
        .highlight_symbol("▸ ");
    let mut state = ListState::default().with_selected(Some(app.picker_selected));
    f.render_stateful_widget(list, area, &mut state);
}
