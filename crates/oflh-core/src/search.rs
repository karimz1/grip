//! Compiled queries and reusable scratch space. Search indices belong to a snapshot.
use crate::{Process, Usage};
#[derive(Debug)]
/// A searchable field with cached text and word boundaries.
pub struct Field {
    lower: String,
    chars: Vec<char>,
    boundaries: Vec<bool>,
    name: bool,
}
impl Field {
    /// Build a field, optionally giving it filename ranking priority.
    pub fn new(text: &str, name: bool) -> Self {
        let chars: Vec<_> = text.chars().collect();
        let boundaries = (0..chars.len())
            .map(|i| {
                let character = chars[i];
                i == 0
                    || !chars[i - 1].is_alphanumeric()
                    || character.is_uppercase()
                        && (chars[i - 1].is_lowercase()
                            || chars.get(i + 1).is_some_and(|c| c.is_lowercase()))
                    || !character.is_alphanumeric()
            })
            .collect();
        Self {
            lower: text.to_lowercase(),
            chars,
            boundaries,
            name,
        }
    }
}
#[derive(Default)]
/// Reusable matching buffers shared across fields and queries.
pub struct Scratch {
    previous: Vec<bool>,
    next: Vec<bool>,
}
#[derive(Clone, Debug)]
struct Term {
    text: String,
    chunks: Vec<Vec<char>>,
}
#[derive(Clone, Debug, Default)]
/// A compiled, whitespace-separated search query.
pub struct Query {
    terms: Vec<Term>,
}
impl Query {
    /// Compile substring, wildcard, and word-boundary search terms.
    pub fn new(text: &str) -> Self {
        Self {
            terms: text
                .split_whitespace()
                .map(|term| {
                    let text = term.to_lowercase();
                    let chunks = text
                        .split('*')
                        .map(|chunk| chunk.chars().collect())
                        .collect();
                    Term { text, chunks }
                })
                .collect(),
        }
    }
    /// Whether the query contains no terms.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
    /// Check that every query term matches at least one field.
    pub fn matches(&self, fields: &[Field], scratch: &mut Scratch) -> bool {
        self.terms
            .iter()
            .all(|t| fields.iter().any(|field| t.matches(field, scratch)))
    }
    /// Rank matching fields, preferring exact names and prefixes.
    pub fn score(&self, fields: &[Field], scratch: &mut Scratch) -> u32 {
        self.terms
            .iter()
            .map(|term| term.best_score(fields.iter(), scratch))
            .sum()
    }

    /// Keep terms not already satisfied by process metadata.
    pub fn file_terms(&self, metadata: &[Field], scratch: &mut Scratch) -> Self {
        Self {
            terms: self
                .terms
                .iter()
                .filter(|t| !metadata.iter().any(|field| t.matches(field, scratch)))
                .cloned()
                .collect(),
        }
    }
    /// Return the normalized query text.
    pub fn text(&self) -> String {
        self.terms
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
impl Term {
    fn best_score<'a>(
        &self,
        fields: impl Iterator<Item = &'a Field>,
        scratch: &mut Scratch,
    ) -> u32 {
        fields
            .map(|field| self.score(field, scratch))
            .max()
            .unwrap_or(0)
    }

    fn score(&self, field: &Field, scratch: &mut Scratch) -> u32 {
        let score = if field.lower == self.text {
            100
        } else if field.lower.starts_with(&self.text) {
            80
        } else if field.lower.contains(&self.text) {
            60
        } else if self.matches(field, scratch) {
            30
        } else {
            0
        };
        score * if field.name { 2 } else { 1 }
    }

    fn matches(&self, field: &Field, scratch: &mut Scratch) -> bool {
        if self.chunks.len() == 1 && field.lower.contains(&self.text) {
            return true;
        }
        let mut start = 0;
        for chunk in &self.chunks {
            match match_end(chunk, field, start, scratch) {
                Some(end) => start = end,
                None => return false,
            }
        }
        true
    }
}
fn lower(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}
/// Find the earliest matching end, allowing abbreviation gaps only at word boundaries
/// within one path component. Reuse dynamic-programming buffers across terms.
fn match_end(query: &[char], field: &Field, start: usize, scratch: &mut Scratch) -> Option<usize> {
    if query.is_empty() {
        return Some(start);
    }
    let chars = &field.chars;
    let mut best = chars.len() + 1;
    if query.len() <= chars.len().saturating_sub(start) {
        for i in start..=chars.len() - query.len() {
            if query
                .iter()
                .enumerate()
                .all(|(j, c)| lower(chars[i + j]) == *c)
            {
                best = i + query.len();
                break;
            }
        }
    }
    scratch.previous.resize(chars.len(), false);
    scratch.previous.fill(false);
    scratch.next.resize(chars.len(), false);
    for (query_index, query_character) in query.iter().enumerate() {
        scratch.next.fill(false);
        let mut earlier = false;
        for (j, &character) in chars.iter().enumerate().take(best).skip(start) {
            if character == '/' || character == '\\' {
                earlier = false;
                continue;
            }
            if lower(character) == *query_character {
                scratch.next[j] = if query_index == 0 {
                    field.boundaries[j]
                } else {
                    j > start && scratch.previous[j - 1] || field.boundaries[j] && earlier
                };
            }
            earlier |= scratch.previous[j];
        }
        std::mem::swap(&mut scratch.previous, &mut scratch.next);
    }
    for j in start..chars.len().min(best) {
        if scratch.previous[j] {
            return Some(j + 1);
        }
    }
    (best <= chars.len()).then_some(best)
}
/// Search fields cached for one process snapshot.
pub struct ProcessIndex {
    /// Process identity and descriptive fields.
    pub metadata: Vec<Field>,
    /// Fields for each observed file usage.
    pub usages: Vec<Vec<Field>>,
}
impl ProcessIndex {
    /// Cache searchable fields for a process and its file usages.
    pub fn new(process: &Process) -> Self {
        Self {
            metadata: vec![
                Field::new(&process.identity.pid.to_string(), false),
                Field::new(&process.name, true),
                Field::new(&process.user, false),
                Field::new(&process.executable.to_string_lossy(), false),
                Field::new(&process.cwd.to_string_lossy(), false),
            ],
            usages: process.usages.iter().map(usage_fields).collect(),
        }
    }
    /// Check the query against process metadata and all usages.
    pub fn matches(&self, query: &Query, scratch: &mut Scratch) -> bool {
        query.terms.iter().all(|t| {
            self.metadata
                .iter()
                .chain(self.usages.iter().flatten())
                .any(|field| t.matches(field, scratch))
        })
    }
    /// Rank this process against a compiled query.
    pub fn score(&self, query: &Query, scratch: &mut Scratch) -> u32 {
        query
            .terms
            .iter()
            .map(|term| {
                term.best_score(
                    self.metadata.iter().chain(self.usages.iter().flatten()),
                    scratch,
                )
            })
            .sum()
    }
}
fn usage_fields(usage: &Usage) -> Vec<Field> {
    let path = usage.path.to_string_lossy();
    let name = path.rsplit(['/', '\\']).next().unwrap_or(&path);
    let mut fields = vec![
        Field::new(&path, false),
        Field::new(name, true),
        Field::new(usage.relation.label(), false),
        Field::new(usage.access.label(), false),
    ];
    if let Some(lock) = &usage.lock {
        fields.push(Field::new(&lock.to_string(), false))
    }
    fields
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantics() {
        for (field, query, want) in [
            ("Microsoft.IdentityModel.JsonWebTokens.dll", "MIMJWT", true),
            ("FileLockExampleCli.deps.json", "FLEC*.json", true),
            ("FileLockExampleCli.dll", "FLEC.", true),
            ("/File/Lock/Example/Cli.dll", "FLEC*", false),
            ("/FileLock/ExampleCli.dll", "FL*EC.dll", true),
            ("/build/Über.dll", "ÜB*dll", true),
            ("/build/a[1].dll", "*[1]*dll", true),
            ("FileLockExampleCli.dll", "FLEC*FLEC", false),
            ("/alpha/beta/gamma.json", "abg", false),
            ("Micro.Core.dll", "mcrdll", false),
            ("Micro.Core.dll", "MiCoDll", true),
        ] {
            assert_eq!(
                Query::new(query).matches(&[Field::new(field, false)], &mut Scratch::default()),
                want,
                "{query} in {field}"
            );
        }
    }
}
