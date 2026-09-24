//! Compiled queries and reusable scratch space. Search indices belong to a snapshot.
use crate::{Process, Usage};
#[derive(Debug)]
pub struct Field {
    lower: String,
    chars: Vec<char>,
    boundaries: Vec<bool>,
    pub name: bool,
}
impl Field {
    pub fn new(text: &str, name: bool) -> Self {
        let chars: Vec<_> = text.chars().collect();
        let boundaries = (0..chars.len())
            .map(|i| {
                let c = chars[i];
                i == 0
                    || !chars[i - 1].is_alphanumeric()
                    || c.is_uppercase()
                        && (chars[i - 1].is_lowercase()
                            || chars.get(i + 1).is_some_and(|c| c.is_lowercase()))
                    || !c.is_alphanumeric()
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
pub struct Scratch {
    previous: Vec<bool>,
    next: Vec<bool>,
}
#[derive(Clone, Debug)]
pub struct Term {
    text: String,
    chunks: Vec<Vec<char>>,
}
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub terms: Vec<Term>,
}
impl Query {
    pub fn new(text: &str) -> Self {
        Self {
            terms: text
                .split_whitespace()
                .map(|s| {
                    let text = s.to_lowercase();
                    let chunks = text.split('*').map(|s| s.chars().collect()).collect();
                    Term { text, chunks }
                })
                .collect(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
    pub fn matches(&self, fields: &[Field], scratch: &mut Scratch) -> bool {
        self.terms
            .iter()
            .all(|t| fields.iter().any(|f| t.matches(f, scratch)))
    }
    pub fn score(&self, fields: &[Field], scratch: &mut Scratch) -> u32 {
        self.terms
            .iter()
            .map(|t| {
                fields
                    .iter()
                    .map(|f| {
                        let s = if f.lower == t.text {
                            100
                        } else if f.lower.starts_with(&t.text) {
                            80
                        } else if f.lower.contains(&t.text) {
                            60
                        } else if t.matches(f, scratch) {
                            30
                        } else {
                            0
                        };
                        s * if f.name { 2 } else { 1 }
                    })
                    .max()
                    .unwrap_or(0)
            })
            .sum()
    }
    pub fn file_terms(&self, metadata: &[Field], scratch: &mut Scratch) -> Self {
        Self {
            terms: self
                .terms
                .iter()
                .filter(|t| !metadata.iter().any(|f| t.matches(f, scratch)))
                .cloned()
                .collect(),
        }
    }
    pub fn text(&self) -> String {
        self.terms
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
impl Term {
    fn matches(&self, f: &Field, s: &mut Scratch) -> bool {
        if self.chunks.len() == 1 && f.lower.contains(&self.text) {
            return true;
        }
        let mut start = 0;
        for chunk in &self.chunks {
            match match_end(chunk, f, start, s) {
                Some(end) => start = end,
                None => return false,
            }
        }
        true
    }
}
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}
fn match_end(q: &[char], f: &Field, start: usize, s: &mut Scratch) -> Option<usize> {
    if q.is_empty() {
        return Some(start);
    }
    let chars = &f.chars;
    let mut best = chars.len() + 1;
    if q.len() <= chars.len().saturating_sub(start) {
        for i in start..=chars.len() - q.len() {
            if q.iter().enumerate().all(|(j, c)| lower(chars[i + j]) == *c) {
                best = i + q.len();
                break;
            }
        }
    }
    s.previous.resize(chars.len(), false);
    s.previous.fill(false);
    s.next.resize(chars.len(), false);
    for (qi, qc) in q.iter().enumerate() {
        s.next.fill(false);
        let mut earlier = false;
        for (j, &c) in chars.iter().enumerate().take(best).skip(start) {
            if c == '/' || c == '\\' {
                earlier = false;
                continue;
            }
            if lower(c) == *qc {
                s.next[j] = if qi == 0 {
                    f.boundaries[j]
                } else {
                    j > start && s.previous[j - 1] || f.boundaries[j] && earlier
                };
            }
            earlier |= s.previous[j];
        }
        std::mem::swap(&mut s.previous, &mut s.next);
    }
    for j in start..chars.len().min(best) {
        if s.previous[j] {
            return Some(j + 1);
        }
    }
    (best <= chars.len()).then_some(best)
}
pub struct ProcessIndex {
    pub metadata: Vec<Field>,
    pub usages: Vec<Vec<Field>>,
}
impl ProcessIndex {
    pub fn new(p: &Process) -> Self {
        Self {
            metadata: vec![
                Field::new(&p.identity.pid.to_string(), false),
                Field::new(&p.name, true),
                Field::new(&p.user, false),
                Field::new(&p.executable.to_string_lossy(), false),
                Field::new(&p.cwd.to_string_lossy(), false),
            ],
            usages: p.usages.iter().map(usage_fields).collect(),
        }
    }
    pub fn matches(&self, q: &Query, s: &mut Scratch) -> bool {
        q.terms.iter().all(|t| {
            self.metadata
                .iter()
                .chain(self.usages.iter().flatten())
                .any(|f| t.matches(f, s))
        })
    }
    pub fn score(&self, q: &Query, s: &mut Scratch) -> u32 {
        q.terms
            .iter()
            .map(|t| {
                let q = Query {
                    terms: vec![t.clone()],
                };
                std::iter::once(&self.metadata)
                    .chain(self.usages.iter())
                    .map(|f| q.score(f, s))
                    .max()
                    .unwrap_or(0)
            })
            .sum()
    }
}
fn usage_fields(u: &Usage) -> Vec<Field> {
    let path = u.path.to_string_lossy();
    let name = path.rsplit(['/', '\\']).next().unwrap_or(&path);
    let mut fields = vec![
        Field::new(&path, false),
        Field::new(name, true),
        Field::new(u.relation.label(), false),
        Field::new(u.access.label(), false),
    ];
    if let Some(lock) = &u.lock {
        fields.push(Field::new(&lock.to_string(), false))
    }
    fields
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantics() {
        for (field, q, want) in [
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
                Query::new(q).matches(&[Field::new(field, false)], &mut Scratch::default()),
                want,
                "{q} in {field}"
            );
        }
    }
}
