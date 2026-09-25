use crossterm::event::{KeyCode as K, KeyEvent, KeyModifiers as M};
use oflh_core::{
    search::{ProcessIndex, Query, Scratch},
    *,
};
use std::collections::HashSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Details,
    Help,
    Confirm,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Relevance,
    Name,
    Pid,
    Memory,
    Cpu,
}
#[derive(Clone, Debug)]
pub struct Row {
    pub process: usize,
    pub usages: Vec<usize>,
    score: u32,
}
#[derive(Clone, Debug)]
pub struct ActionTarget {
    pub identity: Identity,
    pub name: String,
}
#[derive(Clone, Debug)]
pub struct Tree {
    pub nodes: Vec<ActionTarget>,
    pub cursor: usize,
}
#[derive(Debug)]
pub enum Effect {
    None,
    Quit,
    Scan,
    Kill(Vec<Identity>, bool),
    Link(&'static str),
}
pub struct App {
    pub target: Target,
    pub version: String,
    pub snapshot: Snapshot,
    indices: Vec<ProcessIndex>,
    pub rows: Vec<Row>,
    pub cursor: usize,
    pub screen: Screen,
    pub locked: bool,
    pub sort: Sort,
    pub selected: HashSet<Identity>,
    pub query: String,
    pub detail_query: String,
    pub editing: bool,
    edit_before: String,
    pub input_cursor: usize,
    pub detail_id: Option<Identity>,
    pub detail_scope: Option<Usage>,
    pub detail_locks: bool,
    pub usage_rows: Vec<usize>,
    pub usage_cursor: usize,
    pub path_page: usize,
    pub tree: Option<Tree>,
    pub hide_inspector: bool,
    pub auto: bool,
    pub scanning: bool,
    pub stopping: bool,
    pub pulse: usize,
    pub pending: Vec<ActionTarget>,
    pub force: bool,
    pub confirm: bool,
    pub parent_action: bool,
    pub scroll: usize,
    pub status: String,
    pub error: bool,
    pub width: u16,
    pub height: u16,
    pub page: usize,
}
impl App {
    pub fn new(target: Target, version: String) -> Self {
        Self {
            target,
            version,
            snapshot: Snapshot::default(),
            indices: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            screen: Screen::Main,
            locked: false,
            sort: Sort::Relevance,
            selected: HashSet::new(),
            query: String::new(),
            detail_query: String::new(),
            editing: false,
            edit_before: String::new(),
            input_cursor: 0,
            detail_id: None,
            detail_scope: None,
            detail_locks: false,
            usage_rows: Vec::new(),
            usage_cursor: 0,
            path_page: 0,
            tree: None,
            hide_inspector: false,
            auto: false,
            scanning: false,
            stopping: false,
            pulse: 0,
            pending: Vec::new(),
            force: false,
            confirm: false,
            parent_action: false,
            scroll: 0,
            status: String::new(),
            error: false,
            width: 80,
            height: 24,
            page: 10,
        }
    }
    pub fn current(&self) -> Option<&Process> {
        self.rows
            .get(self.cursor)
            .and_then(|row| self.snapshot.processes.get(row.process))
    }
    pub fn detail(&self) -> Option<&Process> {
        self.detail_id.and_then(|identity| {
            self.snapshot
                .processes
                .iter()
                .find(|process| process.identity == identity)
        })
    }
    pub fn replace(&mut self, snapshot: Snapshot) {
        let selected = self.current().map(|process| process.identity);
        let usage = self
            .rows
            .get(self.cursor)
            .filter(|_| self.locked)
            .and_then(|row| {
                self.snapshot.processes[row.process]
                    .usages
                    .get(row.usages[0])
            })
            .cloned();
        let detail_usage = self
            .detail()
            .and_then(|process| {
                self.usage_rows
                    .get(self.usage_cursor)
                    .and_then(|&i| process.usages.get(i))
            })
            .cloned();
        self.snapshot = snapshot;
        self.indices = self
            .snapshot
            .processes
            .iter()
            .map(ProcessIndex::new)
            .collect();
        self.selected.retain(|identity| {
            self.snapshot
                .processes
                .iter()
                .any(|process| process.identity == *identity)
        });
        self.refilter();
        if let Some(position) = self.rows.iter().position(|row| {
            Some(self.snapshot.processes[row.process].identity) == selected
                && usage.as_ref().is_none_or(|usage| {
                    row.usages
                        .iter()
                        .any(|&i| self.snapshot.processes[row.process].usages[i] == *usage)
                })
        }) {
            self.cursor = position
        }
        self.filter_details();
        if let Some(process) = self.detail()
            && let Some(position) = self
                .usage_rows
                .iter()
                .position(|&i| Some(&process.usages[i]) == detail_usage.as_ref())
        {
            self.usage_cursor = position
        }
    }
    pub fn metrics(&mut self, metrics: Vec<(Identity, Metrics)>) {
        for (identity, m) in metrics {
            if let Some(process) = self
                .snapshot
                .processes
                .iter_mut()
                .find(|process| process.identity == identity)
            {
                process.memory = m.memory;
                process.cpu = m.cpu
            }
        }
        if matches!(self.sort, Sort::Cpu | Sort::Memory) {
            let identity = self.current().map(|process| process.identity);
            self.sort_rows();
            if let Some(i) = self
                .rows
                .iter()
                .position(|row| Some(self.snapshot.processes[row.process].identity) == identity)
            {
                self.cursor = i
            }
        }
    }
    pub fn refilter(&mut self) {
        let query = Query::new(&self.query);
        let mut scratch = Scratch::default();
        self.rows.clear();
        for (i, process) in self.snapshot.processes.iter().enumerate() {
            let index = &self.indices[i];
            if self.locked {
                for (j, usage) in process.usages.iter().enumerate() {
                    if usage.lock.is_none() {
                        continue;
                    }
                    let file_q = query.file_terms(&index.metadata, &mut scratch);
                    if file_q.matches(&index.usages[j], &mut scratch) {
                        let score = query
                            .score(&index.metadata, &mut scratch)
                            .max(query.score(&index.usages[j], &mut scratch));
                        self.rows.push(Row {
                            process: i,
                            usages: vec![j],
                            score,
                        })
                    }
                }
            } else if index.matches(&query, &mut scratch) {
                let file_q = query.file_terms(&index.metadata, &mut scratch);
                let usages: Vec<_> = index
                    .usages
                    .iter()
                    .enumerate()
                    .filter_map(|(j, fields)| file_q.matches(fields, &mut scratch).then_some(j))
                    .collect();
                if !usages.is_empty() {
                    self.rows.push(Row {
                        process: i,
                        usages,
                        score: index.score(&query, &mut scratch),
                    })
                }
            }
        }
        self.sort_rows();
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
    }
    fn sort_rows(&mut self) {
        let processes = &self.snapshot.processes;
        let sort = self.sort;
        self.rows.sort_by(|a, b| {
            let left_process = &processes[a.process];
            let right_process = &processes[b.process];
            let ordering = match sort {
                Sort::Relevance => b.score.cmp(&a.score),
                Sort::Name => left_process
                    .name
                    .to_lowercase()
                    .cmp(&right_process.name.to_lowercase()),
                Sort::Pid => left_process.identity.pid.cmp(&right_process.identity.pid),
                Sort::Memory => right_process.memory.cmp(&left_process.memory),
                Sort::Cpu => right_process
                    .cpu
                    .partial_cmp(&left_process.cpu)
                    .unwrap_or(std::cmp::Ordering::Equal),
            };
            ordering.then(left_process.identity.pid.cmp(&right_process.identity.pid))
        });
    }
    pub fn filter_details(&mut self) {
        self.usage_rows.clear();
        let Some(identity) = self.detail_id else {
            return;
        };
        let Some(i) = self
            .snapshot
            .processes
            .iter()
            .position(|process| process.identity == identity)
        else {
            return;
        };
        let process = &self.snapshot.processes[i];
        let query = Query::new(&self.detail_query);
        let mut scratch = Scratch::default();
        let mut ranked = Vec::new();
        for (j, usage) in process.usages.iter().enumerate() {
            if self.detail_locks && usage.lock.is_none()
                || self
                    .detail_scope
                    .as_ref()
                    .is_some_and(|scope| scope != usage)
            {
                continue;
            }
            let fields = &self.indices[i].usages[j];
            if query.matches(fields, &mut scratch) {
                ranked.push((j, query.score(fields, &mut scratch)))
            }
        }
        ranked.sort_by_key(|&(_, text)| std::cmp::Reverse(text));
        self.usage_rows = ranked.into_iter().map(|(i, _)| i).collect();
        self.usage_cursor = self
            .usage_cursor
            .min(self.usage_rows.len().saturating_sub(1));
        self.path_page = 0;
    }
    pub fn paste(&mut self, text: &str) {
        if !self.editing {
            return;
        }
        for c in text.chars().filter(|c| !c.is_control()) {
            self.insert(c)
        }
    }
    fn input(&self) -> &str {
        if self.screen == Screen::Details {
            &self.detail_query
        } else {
            &self.query
        }
    }
    fn input_mut(&mut self) -> &mut String {
        if self.screen == Screen::Details {
            &mut self.detail_query
        } else {
            &mut self.query
        }
    }
    fn changed(&mut self) {
        if self.screen == Screen::Details {
            self.usage_cursor = 0;
            self.filter_details()
        } else {
            self.cursor = 0;
            self.refilter()
        }
    }
    fn insert(&mut self, character: char) {
        if self.input().chars().count() >= 256 {
            return;
        }
        let position = self.input_cursor;
        self.input_mut().insert(position, character);
        self.input_cursor += character.len_utf8();
        self.changed()
    }
    fn begin_search(&mut self) {
        self.editing = true;
        self.edit_before = self.input().to_owned();
        self.input_cursor = self.input().len()
    }
    fn edit_key(&mut self, key: KeyEvent) {
        match key.code {
            K::Enter => self.editing = false,
            K::Esc => {
                let before = self.edit_before.clone();
                *self.input_mut() = before;
                self.editing = false;
                self.changed()
            }
            K::Backspace => {
                let position = self.input_cursor;
                if let Some((prev, _)) = self.input()[..position].char_indices().next_back() {
                    self.input_mut().drain(prev..position);
                    self.input_cursor = prev;
                    self.changed()
                }
            }
            K::Delete => {
                let position = self.input_cursor;
                if let Some(character) = self.input()[position..].chars().next() {
                    self.input_mut()
                        .drain(position..position + character.len_utf8());
                    self.changed()
                }
            }
            K::Left => {
                self.input_cursor = self.input()[..self.input_cursor]
                    .char_indices()
                    .next_back()
                    .map_or(0, |(i, _)| i)
            }
            K::Right => {
                if let Some(character) = self.input()[self.input_cursor..].chars().next() {
                    self.input_cursor += character.len_utf8()
                }
            }
            K::Home => self.input_cursor = 0,
            K::End => self.input_cursor = self.input().len(),
            K::Up | K::Down => self.navigate(key.code),
            K::Char('u') if key.modifiers.contains(M::CONTROL) => {
                self.input_mut().clear();
                self.input_cursor = 0;
                self.changed()
            }
            K::Char(character) if !key.modifiers.intersects(M::CONTROL | M::ALT) => {
                self.insert(character)
            }
            _ => {}
        }
    }

    fn help_key(&mut self, key: K) -> Effect {
        match key {
            K::Esc | K::Char('q') | K::Char('?') => {
                self.screen = Screen::Main;
                self.scroll = 0
            }
            K::Up => self.scroll = self.scroll.saturating_sub(1),
            K::Down | K::Char('j') => self.scroll += 1,
            K::PageDown => self.scroll += self.page,
            K::PageUp => self.scroll = self.scroll.saturating_sub(self.page),
            K::Char('R') => {
                return Effect::Link("https://github.com/karimz1/open-file-lock-handle");
            }
            K::Char('D') => return Effect::Link("https://buymeacoffee.com/karimz1"),
            _ => {}
        }
        Effect::None
    }

    pub fn key(&mut self, key: KeyEvent) -> Effect {
        if key.modifiers.contains(M::CONTROL) && key.code == K::Char('c') {
            return Effect::Quit;
        }
        if self.editing {
            self.edit_key(key);
            return Effect::None;
        }
        if self.screen == Screen::Confirm {
            return self.confirm_key(key.code);
        }
        if self.screen == Screen::Help {
            return self.help_key(key.code);
        }
        if let Some(tree) = &mut self.tree {
            match key.code {
                K::Esc | K::Left | K::Tab | K::BackTab | K::Char('i') => self.tree = None,
                K::Up => tree.cursor = tree.cursor.saturating_sub(1),
                K::Down | K::Char('j') => {
                    tree.cursor = (tree.cursor + 1).min(tree.nodes.len().saturating_sub(1))
                }
                K::Home => tree.cursor = 0,
                K::End => tree.cursor = tree.nodes.len().saturating_sub(1),
                K::Char('k' | 'x') => {
                    if self.width < 38 || self.height < 22 {
                        self.status =
                            "Enlarge the terminal to review the selected ancestor.".into();
                        self.error = true
                    } else {
                        self.prepare_kill(key.code)
                    }
                }
                K::Char('1' | '2') => {
                    self.tree = None;
                    self.locked = key.code == K::Char('2');
                    self.cursor = 0;
                    self.refilter()
                }
                K::Char('q') => return Effect::Quit,
                _ => {}
            }
            return Effect::None;
        }
        match key.code {
            K::Char('q') => {
                if self.screen == Screen::Details {
                    self.screen = Screen::Main
                } else {
                    return Effect::Quit;
                }
            }
            K::Char('/') => self.begin_search(),
            K::Char('r') => {
                if !self.stopping {
                    return Effect::Scan;
                }
            }
            K::Char('a') => self.auto = !self.auto,
            K::Char('k' | 'K' | 'x' | 'X') => self.prepare_kill(key.code),
            K::Char('?') => {
                self.screen = Screen::Help;
                self.scroll = 0
            }
            K::Char('R') => {
                return Effect::Link("https://github.com/karimz1/open-file-lock-handle");
            }
            K::Char('D') => return Effect::Link("https://buymeacoffee.com/karimz1"),
            K::Esc => {
                if self.screen == Screen::Details {
                    if !self.detail_query.is_empty() {
                        self.detail_query.clear();
                        self.filter_details()
                    } else {
                        self.screen = Screen::Main
                    }
                } else if !self.query.is_empty() {
                    self.query.clear();
                    self.refilter()
                } else {
                    self.selected.clear()
                }
            }
            K::Up
            | K::Down
            | K::PageUp
            | K::PageDown
            | K::Home
            | K::End
            | K::Char('j' | 'g' | 'G') => self.navigate(key.code),
            _ if self.screen == Screen::Details => match key.code {
                K::Char('l') => {
                    self.detail_locks = !self.detail_locks;
                    self.filter_details()
                }
                K::Left => self.path_page = self.path_page.saturating_sub(1),
                K::Right => self.path_page = self.path_page.saturating_add(1),
                _ => {}
            },
            K::Enter => self.open_details(),
            K::Char('1' | '2') => {
                self.locked = key.code == K::Char('2');
                self.cursor = 0;
                self.refilter()
            }
            K::Char(' ') => {
                if let Some(identity) = self.current().map(|process| process.identity)
                    && !self.selected.remove(&identity)
                {
                    self.selected.insert(identity);
                }
            }
            K::Char('i') => self.hide_inspector = !self.hide_inspector,
            K::Tab | K::Right => {
                if let Some(process) = self.current() {
                    let mut nodes: Vec<_> = process
                        .ancestors
                        .iter()
                        .rev()
                        .map(|a| ActionTarget {
                            identity: a.identity,
                            name: a.name.clone(),
                        })
                        .collect();
                    nodes.push(ActionTarget {
                        identity: process.identity,
                        name: process.name.clone(),
                    });
                    let cursor = nodes.len() - 1;
                    self.tree = Some(Tree { nodes, cursor })
                }
            }
            K::Char('n' | 'p' | 'm' | 'c') => {
                self.sort = match key.code {
                    K::Char('n') => Sort::Name,
                    K::Char('m') => Sort::Memory,
                    K::Char('c') => Sort::Cpu,
                    _ => Sort::Pid,
                };
                self.refilter()
            }
            _ => {}
        }
        Effect::None
    }
    pub fn select_all(&mut self) {
        let all = self.rows.iter().all(|row| {
            self.selected
                .contains(&self.snapshot.processes[row.process].identity)
        });
        for row in &self.rows {
            let identity = self.snapshot.processes[row.process].identity;
            if all {
                self.selected.remove(&identity);
            } else {
                self.selected.insert(identity);
            }
        }
    }
    fn navigate(&mut self, key: K) {
        let (len, cursor) = if self.screen == Screen::Details {
            (self.usage_rows.len(), &mut self.usage_cursor)
        } else {
            (self.rows.len(), &mut self.cursor)
        };
        *cursor = match key {
            K::Up => cursor.saturating_sub(1),
            K::Down | K::Char('j') => cursor.saturating_add(1),
            K::PageUp => cursor.saturating_sub(self.page),
            K::PageDown => cursor.saturating_add(self.page),
            K::Home | K::Char('g') => 0,
            K::End | K::Char('G') => len.saturating_sub(1),
            _ => *cursor,
        }
        .min(len.saturating_sub(1));
        self.path_page = 0;
    }
    fn open_details(&mut self) {
        let Some(row) = self.rows.get(self.cursor) else {
            return;
        };
        let process = &self.snapshot.processes[row.process];
        self.detail_id = Some(process.identity);
        self.detail_scope = if self.locked {
            Some(process.usages[row.usages[0]].clone())
        } else {
            None
        };
        self.detail_query = Query::new(&self.query)
            .file_terms(&self.indices[row.process].metadata, &mut Scratch::default())
            .text();
        self.detail_locks = false;
        self.usage_cursor = 0;
        self.screen = Screen::Details;
        self.filter_details()
    }
    fn prepare_kill(&mut self, key: K) {
        if self.stopping {
            return;
        }
        self.pending.clear();
        self.confirm = false;
        self.scroll = 0;
        self.force = matches!(key, K::Char('x' | 'X'));
        self.parent_action = false;
        if let Some(tree) = &self.tree {
            let target = tree.nodes[tree.cursor].clone();
            if target.identity.validate().is_err() {
                self.status =
                    "This ancestor cannot be terminated: protected or identity unavailable.".into();
                self.error = true;
                return;
            }
            self.parent_action = tree.cursor + 1 < tree.nodes.len();
            self.pending.push(target)
        } else if self.screen == Screen::Details {
            if let Some(process) = self.detail() {
                self.pending.push(ActionTarget {
                    identity: process.identity,
                    name: process.name.clone(),
                })
            }
        } else if !self.selected.is_empty() {
            self.pending.extend(
                self.snapshot
                    .processes
                    .iter()
                    .filter(|process| self.selected.contains(&process.identity))
                    .map(|process| ActionTarget {
                        identity: process.identity,
                        name: process.name.clone(),
                    }),
            )
        } else if matches!(key, K::Char('K' | 'X')) {
            let mut seen = HashSet::new();
            for row in &self.rows {
                let process = &self.snapshot.processes[row.process];
                if seen.insert(process.identity) {
                    self.pending.push(ActionTarget {
                        identity: process.identity,
                        name: process.name.clone(),
                    })
                }
            }
        } else if let Some(process) = self.current() {
            self.pending.push(ActionTarget {
                identity: process.identity,
                name: process.name.clone(),
            })
        }
        if !self.pending.is_empty() {
            self.screen = Screen::Confirm
        }
    }
    fn confirm_key(&mut self, key: K) -> Effect {
        match key {
            K::Esc | K::Char('q' | 'n') => {
                self.pending.clear();
                self.screen = Screen::Main
            }
            K::Tab | K::Left | K::Right => self.confirm = !self.confirm,
            K::Up => self.scroll = self.scroll.saturating_sub(1),
            K::Down | K::Char('j') => self.scroll += 1,
            K::PageUp => self.scroll = self.scroll.saturating_sub(self.page),
            K::PageDown => self.scroll += self.page,
            K::Enter => {
                if !self.confirm {
                    self.pending.clear();
                    self.screen = Screen::Main
                } else if self.width < 38 || self.height < 12 {
                    self.status =
                        "Enlarge the terminal to review targets before confirming.".into();
                    self.error = true
                } else {
                    self.screen = Screen::Main;
                    self.stopping = true;
                    self.status = "Requesting termination…".into();
                    return Effect::Kill(
                        self.pending
                            .drain(..)
                            .map(|process| process.identity)
                            .collect(),
                        self.force,
                    );
                }
            }
            _ => {}
        }
        Effect::None
    }
}
