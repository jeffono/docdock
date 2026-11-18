//! Spell checking for the editor. The Windows checkers live on one thread, opened on first use; a
//! word is misspelled only when every language flags it, so notes may mix languages.
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use crate::platform::spell::Checkers;

/// Longer words are left unchecked, as are words beyond this many in one request.
const MAX_WORD: usize = 64;
const MAX_WORDS: usize = 1000;
const MAX_SUGGESTIONS: usize = 6;
/// A stuck checker must not hold a command's thread; its answer then reads as the default.
const ANSWER_WAIT: Duration = Duration::from_secs(5);

enum Job {
    Languages(Sender<Vec<String>>),
    Check(Vec<String>, Sender<Vec<bool>>),
    Suggest(String, Sender<Vec<String>>),
    Add(String, Sender<bool>),
}

pub struct Spelling(Sender<Job>);

impl Spelling {
    pub fn start() -> Self {
        let (jobs, received) = channel::<Job>();
        let spawned = std::thread::Builder::new()
            .name("spelling".into())
            .spawn(move || {
                let mut checkers: Option<Checkers> = None;
                for job in received {
                    let checkers = checkers.get_or_insert_with(Checkers::open);
                    match job {
                        Job::Languages(reply) => _ = reply.send(checkers.languages()),
                        Job::Check(words, reply) => _ = reply.send(check(checkers, &words)),
                        Job::Suggest(word, reply) => _ = reply.send(suggest(checkers, &word)),
                        Job::Add(word, reply) => {
                            let added = checkers.add(&word);
                            if let Err(error) = &added {
                                eprintln!("[spelling] add: {error}");
                            }
                            _ = reply.send(added.is_ok());
                        }
                    }
                }
            });
        if let Err(error) = spawned {
            eprintln!("[spelling] {error}");
        }
        Self(jobs)
    }

    /// Sends a job and waits for its answer; a stopped or stuck thread answers with the default.
    fn ask<T: Default>(&self, job: impl FnOnce(Sender<T>) -> Job) -> T {
        let (reply, answer) = channel();
        if self.0.send(job(reply)).is_err() {
            return T::default();
        }
        answer.recv_timeout(ANSWER_WAIT).unwrap_or_default()
    }

    pub fn languages(&self) -> Vec<String> {
        self.ask(Job::Languages)
    }
    /// One flag per word, true when misspelled.
    pub fn check(&self, words: Vec<String>) -> Vec<bool> {
        self.ask(|reply| Job::Check(words, reply))
    }
    pub fn suggest(&self, word: String) -> Vec<String> {
        self.ask(|reply| Job::Suggest(word, reply))
    }
    /// Whether the word reached the dictionary.
    pub fn add(&self, word: String) -> bool {
        self.ask(|reply| Job::Add(word, reply))
    }
}

fn check(checkers: &Checkers, words: &[String]) -> Vec<bool> {
    words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            index < MAX_WORDS
                && checkable(word)
                && checkers.count() > 0
                && (0..checkers.count()).all(|language| checkers.flags(language, word))
        })
        .collect()
}

fn checkable(word: &str) -> bool {
    !word.is_empty() && word.chars().count() <= MAX_WORD && !word.chars().any(char::is_whitespace)
}

fn suggest(checkers: &Checkers, word: &str) -> Vec<String> {
    if !checkable(word) {
        return Vec::new();
    }
    interleave(
        (0..checkers.count())
            .map(|language| checkers.suggest(language, word))
            .collect(),
        MAX_SUGGESTIONS,
    )
}

/// Each language's best suggestion first, then each one's second and so on, without repeats.
fn interleave(lists: Vec<Vec<String>>, limit: usize) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
    for rank in 0..longest {
        for list in &lists {
            if merged.len() == limit {
                return merged;
            }
            if let Some(item) = list.get(rank)
                && !merged.contains(item)
            {
                merged.push(item.clone());
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn interleave_takes_each_language_in_turn_without_repeats() {
        let merged = interleave(
            vec![words(&["casa", "caso", "cama"]), words(&["case", "casa"])],
            4,
        );
        assert_eq!(merged, words(&["casa", "case", "caso", "cama"]));
    }
}
