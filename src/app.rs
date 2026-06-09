use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use image::DynamicImage;
use ratatui_image::picker::Picker;
use ratatui_image::thread::{ResizeRequest, ThreadProtocol};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::Instant;

use crate::api::client::{MtgClient, SearchQuery};
use crate::api::models::Card;
use crate::deck::{Deck, storage};
use crate::event::AppEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Results,
    Deck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    SearchInput,
    CategoryInput { target: String },
    RenameInput,
    NewDeckInput,
    Help,
    Stats,
    DeckPicker,
}

/// A visual row in the deck panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckRow {
    CommanderHeader,
    CommanderCard,
    CommanderEmpty,
    CategoryHeader { name: String, count: u32 },
    Entry { name: String },
}

pub struct App {
    pub client: MtgClient,
    pub tx: UnboundedSender<AppEvent>,
    rx: UnboundedReceiver<AppEvent>,
    pub should_quit: bool,

    pub focus: Focus,
    pub mode: Mode,
    pub input_buffer: String,

    // Search state
    pub search_input: String,
    pub searching: bool,
    pub query_id: u64,
    pub results: Vec<Card>,
    pub results_selected: usize,
    pub total_count: Option<u64>,
    pub page: u32,
    last_query: Option<SearchQuery>,

    // Deck state
    pub deck: Deck,
    pub deck_selected: usize,
    pub collapsed: HashSet<String>,
    pub dirty: bool,

    // Deck picker overlay
    pub saved_decks: Vec<(String, PathBuf)>,
    pub picker_selected: usize,

    // Image state. Resize+encode is offloaded to a worker task (ThreadProtocol)
    // so the UI thread never blocks; switching cards is debounced so holding
    // j/k doesn't queue an encode/download per card skimmed over.
    pub img_picker: Picker,
    pub image_state: ThreadProtocol,
    resize_rx: Option<UnboundedReceiver<ResizeRequest>>,
    pub image_for: Option<String>,
    image_debounce: Option<(String, Instant)>,
    image_cache: HashMap<String, DynamicImage>,
    image_pending: HashSet<String>,

    /// (message, is_error)
    pub status: Option<(String, bool)>,
}

impl App {
    pub fn new(img_picker: Picker) -> Self {
        let (tx, rx) = unbounded_channel();
        let (resize_tx, resize_rx) = unbounded_channel();
        Self {
            client: MtgClient::new(),
            tx,
            rx,
            should_quit: false,
            focus: Focus::Results,
            mode: Mode::Normal,
            input_buffer: String::new(),
            search_input: String::new(),
            searching: false,
            query_id: 0,
            results: Vec::new(),
            results_selected: 0,
            total_count: None,
            page: 1,
            last_query: None,
            deck: Deck::new("untitled"),
            deck_selected: 0,
            collapsed: HashSet::new(),
            dirty: false,
            saved_decks: Vec::new(),
            picker_selected: 0,
            img_picker,
            image_state: ThreadProtocol::new(resize_tx, None),
            resize_rx: Some(resize_rx),
            image_for: None,
            image_debounce: None,
            image_cache: HashMap::new(),
            image_pending: HashSet::new(),
            status: None,
        }
    }

    pub async fn run(mut self, mut terminal: ratatui::DefaultTerminal) -> anyhow::Result<()> {
        self.spawn_encode_worker();
        let mut events = EventStream::new();
        while !self.should_quit {
            self.ensure_image();
            terminal.draw(|f| crate::ui::draw(f, &mut self))?;
            let debounce_deadline = self.image_debounce.as_ref().map(|(_, t)| *t);
            tokio::select! {
                maybe_event = events.next() => match maybe_event {
                    Some(Ok(event)) => self.on_terminal_event(event),
                    Some(Err(_)) => {}
                    None => break,
                },
                Some(msg) = self.rx.recv() => self.on_app_event(msg),
                // Wake up when the image debounce expires so the pending
                // card's image loads without needing another input event.
                _ = async {
                    match debounce_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                } => {}
            }
            while let Ok(msg) = self.rx.try_recv() {
                self.on_app_event(msg);
            }
        }
        self.autosave();
        Ok(())
    }

    /// Worker that performs image resize+encode off the UI thread.
    /// `ThreadProtocol` drops responses whose id no longer matches, so
    /// encodes for cards the user already scrolled past are discarded.
    fn spawn_encode_worker(&mut self) {
        let Some(mut resize_rx) = self.resize_rx.take() else {
            return;
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            while let Some(request) = resize_rx.recv().await {
                let tx = tx.clone();
                tokio::task::spawn_blocking(move || {
                    if let Ok(response) = request.resize_encode() {
                        let _ = tx.send(AppEvent::ImageEncoded(response));
                    }
                });
            }
        });
    }

    // ----- async events -------------------------------------------------

    fn on_app_event(&mut self, msg: AppEvent) {
        match msg {
            AppEvent::SearchDone { query_id, result } => {
                if query_id != self.query_id {
                    return; // stale response
                }
                self.searching = false;
                match result {
                    Ok(r) => {
                        self.results = r.cards;
                        self.total_count = r.total_count;
                        self.results_selected = 0;
                        let shown = self.results.len();
                        let total = r.total_count.unwrap_or(shown as u64);
                        self.info(format!(
                            "{shown} cards (page {}, {total} printings total)",
                            self.page
                        ));
                    }
                    Err(e) => self.error(format!("Search failed: {e}")),
                }
            }
            AppEvent::ImageDone { card_id, result } => {
                self.image_pending.remove(&card_id);
                match result {
                    Ok(img) => {
                        if self.image_cache.len() > 150 {
                            self.image_cache.clear();
                        }
                        self.image_cache.insert(card_id, img);
                    }
                    Err(e) => self.error(format!("Image load failed: {e}")),
                }
            }
            AppEvent::ImageEncoded(response) => {
                self.image_state.update_resized_protocol(response);
            }
        }
    }

    /// Make sure the image protocol matches the currently selected card.
    /// Cached images swap in immediately (resize+encode happens off-thread);
    /// downloads are debounced so skimming with j/k doesn't hit the API once
    /// per card passed over.
    fn ensure_image(&mut self) {
        const DEBOUNCE: Duration = Duration::from_millis(90);

        let Some(card) = self.detail_card().cloned() else {
            self.clear_image();
            return;
        };
        if card.image_url.is_none() {
            self.clear_image();
            return;
        }
        if self.image_for.as_deref() == Some(card.id.as_str()) {
            return; // already showing this card
        }

        if let Some(img) = self.image_cache.get(&card.id) {
            // Hand the decoded image to ThreadProtocol; the expensive
            // resize+encode runs in the worker, not on the UI thread, and
            // stale results are dropped by id.
            self.image_state
                .replace_protocol(self.img_picker.new_resize_protocol(img.clone()));
            self.image_for = Some(card.id);
            self.image_debounce = None;
            return;
        }

        // Blank the pane so a stale card image is never shown while loading.
        if self.image_for.take().is_some() {
            self.image_state.empty_protocol();
        }

        match &self.image_debounce {
            Some((id, deadline)) if *id == card.id => {
                if Instant::now() < *deadline {
                    return; // user is still skimming
                }
            }
            _ => {
                self.image_debounce = Some((card.id.clone(), Instant::now() + DEBOUNCE));
                return;
            }
        }
        self.image_debounce = None;

        if !self.image_pending.contains(&card.id) {
            self.image_pending.insert(card.id.clone());
            let url = card.image_url.clone().unwrap_or_default();
            let client = self.client.clone();
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let result = crate::images::load_card_image(&client, &card.id, &url)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(AppEvent::ImageDone {
                    card_id: card.id,
                    result,
                });
            });
        }
    }

    fn clear_image(&mut self) {
        self.image_debounce = None;
        if self.image_for.take().is_some() {
            self.image_state.empty_protocol();
        }
    }

    fn start_search(&mut self, page: u32) {
        let query = match &self.last_query {
            Some(q) => {
                let mut q = q.clone();
                q.page = page;
                q
            }
            None => return,
        };
        self.page = page;
        self.query_id += 1;
        self.searching = true;
        let id = self.query_id;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = client.search(&query).await.map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::SearchDone {
                query_id: id,
                result,
            });
        });
    }

    fn submit_search(&mut self) {
        let input = self.search_input.trim().to_string();
        if input.is_empty() {
            return;
        }
        self.last_query = Some(SearchQuery::parse(&input));
        self.start_search(1);
        self.mode = Mode::Normal;
        self.focus = Focus::Results;
    }

    // ----- terminal events ----------------------------------------------

    fn on_terminal_event(&mut self, event: Event) {
        let Event::Key(key) = event else { return };
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        match self.mode.clone() {
            Mode::Normal => self.on_key_normal(key),
            Mode::SearchInput => self.on_key_search_input(key),
            Mode::CategoryInput { target } => self.on_key_category_input(key, &target),
            Mode::RenameInput => self.on_key_simple_input(key, |app, value| {
                if !value.is_empty() {
                    app.deck.name = value;
                    app.dirty = true;
                    app.info("Deck renamed");
                }
            }),
            Mode::NewDeckInput => self.on_key_simple_input(key, |app, value| {
                if !value.is_empty() {
                    app.autosave();
                    app.deck = Deck::new(value);
                    app.deck_selected = 0;
                    app.collapsed.clear();
                    app.dirty = false;
                    app.info("New deck created");
                }
            }),
            Mode::Help | Mode::Stats => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') | KeyCode::Char('S') => {
                    self.mode = Mode::Normal;
                }
                _ => {}
            },
            Mode::DeckPicker => self.on_key_deck_picker(key),
        }
    }

    fn on_key_normal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('S') => self.mode = Mode::Stats,
            KeyCode::Char('/') => self.mode = Mode::SearchInput,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Results => Focus::Deck,
                    Focus::Deck => Focus::Results,
                };
            }
            KeyCode::Char('w') => self.save_deck(),
            KeyCode::Char('E') => self.export_deck(),
            KeyCode::Char('R') => {
                self.input_buffer = self.deck.name.clone();
                self.mode = Mode::RenameInput;
            }
            KeyCode::Char('D') => {
                self.input_buffer.clear();
                self.mode = Mode::NewDeckInput;
            }
            KeyCode::Char('L') => {
                self.saved_decks = storage::list();
                self.picker_selected = 0;
                self.mode = Mode::DeckPicker;
            }
            _ => match self.focus {
                Focus::Results => self.on_key_results(key),
                Focus::Deck => self.on_key_deck(key),
            },
        }
    }

    fn on_key_results(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down
                if !self.results.is_empty() => {
                    self.results_selected =
                        (self.results_selected + 1).min(self.results.len() - 1);
                }
            KeyCode::Char('k') | KeyCode::Up => {
                self.results_selected = self.results_selected.saturating_sub(1);
            }
            KeyCode::Char('g') => self.results_selected = 0,
            KeyCode::Char('G') => {
                self.results_selected = self.results.len().saturating_sub(1);
            }
            KeyCode::Char('a') | KeyCode::Enter => {
                let Some(card) = self.results.get(self.results_selected).cloned() else {
                    return;
                };
                let name = card.name.clone();
                if self.deck.add_card(card) {
                    self.dirty = true;
                    self.info(format!("Added {name}"));
                } else {
                    self.error(format!("{name} is already in the deck (singleton)"));
                }
            }
            KeyCode::Char('C') => {
                let Some(card) = self.results.get(self.results_selected).cloned() else {
                    return;
                };
                if !card.can_be_commander() {
                    self.error(format!("{} cannot be your commander", card.name));
                    return;
                }
                let name = card.name.clone();
                self.deck.set_commander(card);
                self.dirty = true;
                self.info(format!("Commander set: {name}"));
            }
            KeyCode::Char('n')
                if self.has_next_page() => {
                    self.start_search(self.page + 1);
                }
            KeyCode::Char('p')
                if self.page > 1 => {
                    self.start_search(self.page - 1);
                }
            _ => {}
        }
    }

    fn on_key_deck(&mut self, key: KeyEvent) {
        let rows = self.deck_rows();
        if rows.is_empty() {
            return;
        }
        self.deck_selected = self.deck_selected.min(rows.len() - 1);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.deck_selected = (self.deck_selected + 1).min(rows.len() - 1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.deck_selected = self.deck_selected.saturating_sub(1);
            }
            KeyCode::Char('g') => self.deck_selected = 0,
            KeyCode::Char('G') => self.deck_selected = rows.len() - 1,
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let DeckRow::CategoryHeader { name, .. } = &rows[self.deck_selected]
                    && !self.collapsed.remove(name) {
                        self.collapsed.insert(name.clone());
                    }
            }
            KeyCode::Char('x') | KeyCode::Char('d') => match &rows[self.deck_selected] {
                DeckRow::Entry { name } => {
                    let name = name.clone();
                    if self.deck.remove_card(&name) {
                        self.dirty = true;
                        self.info(format!("Removed {name}"));
                    }
                }
                DeckRow::CommanderCard => {
                    self.deck.commander = None;
                    self.dirty = true;
                    self.info("Commander removed");
                }
                _ => {}
            },
            KeyCode::Char('c') => {
                if let DeckRow::Entry { name } = &rows[self.deck_selected] {
                    let target = name.clone();
                    self.input_buffer = self
                        .deck
                        .entries
                        .iter()
                        .find(|e| e.card.name == target)
                        .and_then(|e| e.category.clone())
                        .unwrap_or_default();
                    self.mode = Mode::CategoryInput { target };
                }
            }
            _ => {}
        }
    }

    fn on_key_search_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => self.submit_search(),
            KeyCode::Backspace => {
                self.search_input.pop();
            }
            KeyCode::Char(c) => self.search_input.push(c),
            _ => {}
        }
    }

    fn on_key_category_input(&mut self, key: KeyEvent, target: &str) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let value = self.input_buffer.trim().to_string();
                if value.is_empty() {
                    self.deck.set_category(target, None);
                    self.info(format!("{target}: automatic category"));
                } else {
                    self.deck.set_category(target, Some(value.clone()));
                    self.info(format!("{target} → {value}"));
                }
                self.dirty = true;
                self.input_buffer.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => self.input_buffer.push(c),
            _ => {}
        }
    }

    fn on_key_simple_input(&mut self, key: KeyEvent, apply: fn(&mut Self, String)) {
        match key.code {
            KeyCode::Esc => {
                self.input_buffer.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let value = self.input_buffer.trim().to_string();
                self.input_buffer.clear();
                self.mode = Mode::Normal;
                apply(self, value);
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => self.input_buffer.push(c),
            _ => {}
        }
    }

    fn on_key_deck_picker(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down
                if !self.saved_decks.is_empty() => {
                    self.picker_selected =
                        (self.picker_selected + 1).min(self.saved_decks.len() - 1);
                }
            KeyCode::Char('k') | KeyCode::Up => {
                self.picker_selected = self.picker_selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                let Some((name, path)) = self.saved_decks.get(self.picker_selected).cloned()
                else {
                    return;
                };
                self.autosave();
                match storage::load(&path) {
                    Ok(deck) => {
                        self.deck = deck;
                        self.deck_selected = 0;
                        self.collapsed.clear();
                        self.dirty = false;
                        self.mode = Mode::Normal;
                        self.focus = Focus::Deck;
                        self.info(format!("Loaded {name}"));
                    }
                    Err(e) => self.error(format!("Load failed: {e}")),
                }
            }
            _ => {}
        }
    }

    // ----- helpers --------------------------------------------------------

    pub fn has_next_page(&self) -> bool {
        // total_count counts printings (pre-dedupe); full page heuristic too.
        let page_size = self
            .last_query
            .as_ref()
            .map(|q| q.page_size as usize)
            .unwrap_or(50);
        self.results.len() >= page_size
            || self
                .total_count
                .is_some_and(|t| t > (self.page as u64) * page_size as u64)
    }

    pub fn deck_rows(&self) -> Vec<DeckRow> {
        let mut rows = vec![DeckRow::CommanderHeader];
        rows.push(match &self.deck.commander {
            Some(_) => DeckRow::CommanderCard,
            None => DeckRow::CommanderEmpty,
        });
        for (name, entries) in self.deck.grouped() {
            let count = entries.iter().map(|e| e.quantity).sum();
            rows.push(DeckRow::CategoryHeader {
                name: name.clone(),
                count,
            });
            if !self.collapsed.contains(&name) {
                for entry in entries {
                    rows.push(DeckRow::Entry {
                        name: entry.card.name.clone(),
                    });
                }
            }
        }
        rows
    }

    /// The card shown in the detail pane, based on focus + selection.
    pub fn detail_card(&self) -> Option<&Card> {
        match self.focus {
            Focus::Results => self.results.get(self.results_selected),
            Focus::Deck => {
                let rows = self.deck_rows();
                match rows.get(self.deck_selected)? {
                    DeckRow::CommanderCard => self.deck.commander.as_ref(),
                    DeckRow::Entry { name } => self
                        .deck
                        .entries
                        .iter()
                        .find(|e| &e.card.name == name)
                        .map(|e| &e.card),
                    _ => None,
                }
            }
        }
    }

    fn save_deck(&mut self) {
        match storage::save(&self.deck) {
            Ok(path) => {
                self.dirty = false;
                self.info(format!("Saved to {}", path.display()));
            }
            Err(e) => self.error(format!("Save failed: {e}")),
        }
    }

    fn export_deck(&mut self) {
        match storage::export_txt_to_file(&self.deck) {
            Ok(path) => self.info(format!("Exported to {}", path.display())),
            Err(e) => self.error(format!("Export failed: {e}")),
        }
    }

    fn autosave(&mut self) {
        if self.dirty && (self.deck.commander.is_some() || !self.deck.entries.is_empty()) {
            let _ = storage::save(&self.deck);
            self.dirty = false;
        }
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), false));
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), true));
    }
}
