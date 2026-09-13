use std::sync::Arc;

use bevy::prelude::Resource;

pub trait ArgCompleter: Send + Sync {
    fn complete(&self, prefix: &str) -> Vec<String>;
}

#[derive(Clone)]
pub struct StaticCompleter {
    values: Arc<[String]>,
}

impl StaticCompleter {
    pub fn new(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            values: values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
        }
    }
}

impl ArgCompleter for StaticCompleter {
    fn complete(&self, prefix: &str) -> Vec<String> {
        rank(&self.values, prefix)
    }
}

impl<F> ArgCompleter for F
where
    F: Fn(&str) -> Vec<String> + Send + Sync,
{
    fn complete(&self, prefix: &str) -> Vec<String> {
        self(prefix)
    }
}

#[derive(Clone)]
pub struct CommandSpec {
    pub name: String,
    pub aliases: Vec<String>,
    pub usage: String,
    pub args: Vec<Arc<dyn ArgCompleter>>,
}

impl CommandSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            usage: String::new(),
            args: Vec::new(),
        }
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    pub fn usage(mut self, usage: impl Into<String>) -> Self {
        self.usage = usage.into();
        self
    }

    pub fn arg(mut self, completer: impl ArgCompleter + 'static) -> Self {
        self.args.push(Arc::new(completer));
        self
    }
}

#[derive(Resource, Default, Clone)]
pub struct ConsoleRegistry {
    commands: Vec<CommandSpec>,
}

impl ConsoleRegistry {
    pub fn register(&mut self, spec: CommandSpec) {
        self.commands.push(spec);
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .commands
            .iter()
            .flat_map(|spec| std::iter::once(spec.name.clone()).chain(spec.aliases.iter().cloned()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn help_lines(&self) -> Vec<String> {
        self.commands
            .iter()
            .map(|spec| {
                if spec.usage.is_empty() {
                    spec.name.clone()
                } else {
                    format!("{} — {}", spec.name, spec.usage)
                }
            })
            .collect()
    }

    pub fn resolve(&self, name: &str) -> Option<&CommandSpec> {
        self.commands
            .iter()
            .find(|spec| spec.name == name || spec.aliases.iter().any(|alias| alias == name))
    }

    pub fn suggestions(&self, line: &str, caret: usize) -> Vec<String> {
        let chars: Vec<char> = line.chars().collect();
        let caret = caret.min(chars.len());
        let before: String = chars[..caret].iter().collect();
        let trailing_space = before.ends_with(|c: char| c.is_whitespace());
        let tokens: Vec<&str> = before.split_whitespace().collect();

        if tokens.is_empty() || (tokens.len() == 1 && !trailing_space) {
            let prefix = tokens.first().copied().unwrap_or("").to_ascii_lowercase();

            if prefix.is_empty() {
                return Vec::new();
            }
            return rank(&self.names(), &prefix);
        }

        let command = tokens[0];
        let Some(spec) = self.resolve(command) else {
            return Vec::new();
        };
        let arg_index = if trailing_space {
            tokens.len() - 1
        } else {
            tokens.len().saturating_sub(2)
        };
        let prefix = if trailing_space {
            ""
        } else {
            tokens.last().copied().unwrap_or("")
        };
        spec.args
            .get(arg_index)
            .map(|completer| completer.complete(prefix))
            .unwrap_or_default()
    }
}

fn rank(values: &[String], query: &str) -> Vec<String> {
    let query = query.to_ascii_lowercase();
    if query.is_empty() {
        return values.to_vec();
    }
    let documents: Vec<Vec<String>> = values
        .iter()
        .map(|value| ngrams(&value.to_ascii_lowercase()))
        .collect();
    let terms = ngrams(&query);
    let chars: Vec<char> = query.chars().collect();
    let bigrams: Vec<String> = chars.windows(2).map(|pair| pair.iter().collect()).collect();
    let required_bigrams = bigrams.len().min(2);
    let document_frequencies: Vec<usize> = terms
        .iter()
        .map(|term| {
            documents
                .iter()
                .filter(|candidate| candidate.iter().any(|candidate| candidate == term))
                .count()
        })
        .collect();
    let average_length =
        documents.iter().map(Vec::len).sum::<usize>() as f32 / documents.len().max(1) as f32;
    let k1 = 1.2;
    let b = 0.75;
    let mut ranked: Vec<(bool, f32, &String)> = values
        .iter()
        .zip(&documents)
        .filter_map(|(value, document)| {
            let matching_bigrams = bigrams
                .iter()
                .filter(|term| document.iter().any(|candidate| candidate == *term))
                .count();
            if matching_bigrams < required_bigrams {
                return None;
            }
            let mut score = 0.0;
            for (term, containing) in terms.iter().zip(&document_frequencies) {
                let frequency = document
                    .iter()
                    .filter(|candidate| *candidate == term)
                    .count();
                if frequency == 0 {
                    continue;
                }
                let idf = (1.0
                    + (documents.len() as f32 - *containing as f32 + 0.5)
                        / (*containing as f32 + 0.5))
                    .ln();
                let length = document.len() as f32;
                score += idf * frequency as f32 * (k1 + 1.0)
                    / (frequency as f32 + k1 * (1.0 - b + b * length / average_length));
            }
            (score > 0.0).then_some((value.to_ascii_lowercase().starts_with(&query), score, value))
        })
        .collect();
    ranked.sort_by(
        |(left_prefix, left_score, left), (right_prefix, right_score, right)| {
            right_prefix
                .cmp(left_prefix)
                .then_with(|| {
                    if *left_prefix {
                        left.cmp(right)
                    } else {
                        right_score.total_cmp(left_score)
                    }
                })
                .then_with(|| left.cmp(right))
        },
    );
    ranked
        .into_iter()
        .map(|(_, _, value)| value.clone())
        .collect()
}

fn ngrams(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut terms: Vec<String> = chars
        .iter()
        .map(|character| character.to_string())
        .collect();
    terms.extend(chars.windows(2).map(|pair| pair.iter().collect::<String>()));
    terms
}
