use bevy::prelude::{Message, Resource};

#[derive(Debug, Clone, PartialEq, Eq, Message)]
pub struct ConsoleCommand {
    pub name: String,
    pub args: Vec<String>,
    pub raw: String,

    pub background: bool,

    pub interactive: bool,
}

impl ConsoleCommand {
    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim().strip_prefix('/').unwrap_or(line.trim()).trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut words = command_words(trimmed);

        let background = matches!(words.last(), Some((word, false)) if word == "&");
        if background {
            words.pop();
        }

        let bang_token = matches!(words.last(), Some((word, false)) if word == "!");
        if bang_token {
            words.pop();
        }
        let words: Vec<String> = words.into_iter().map(|(word, _)| word).collect();
        let raw = trimmed.to_owned();
        if raw.is_empty() {
            return None;
        }
        let mut words = words.into_iter();

        let mut name = words.next()?;
        let bang_name = name.ends_with('!') && name.len() > 1;
        if bang_name {
            name.pop();
        }
        Some(Self {
            name,
            args: words.collect(),
            raw,
            background,
            interactive: bang_token || bang_name,
        })
    }

    pub fn parse_script(line: &str) -> Vec<Self> {
        let mut commands = Vec::new();
        let mut quoted = false;
        let mut start = 0;
        let mut chars = line.char_indices().peekable();
        while let Some((index, ch)) = chars.next() {
            match ch {
                '\\' if quoted && chars.peek().is_some_and(|(_, c)| matches!(c, '\\' | '"')) => {
                    chars.next();
                }
                '"' => quoted = !quoted,
                ';' if !quoted => {
                    if let Some(command) = Self::parse(&line[start..index]) {
                        commands.push(command);
                    }
                    start = index + 1;
                }
                _ => {}
            }
        }
        if let Some(command) = Self::parse(&line[start..]) {
            commands.push(command);
        }
        commands
    }
}

pub type SubmittedCommand = ConsoleCommand;

fn command_words(line: &str) -> Vec<(String, bool)> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut word = String::new();
        let mut quoted = false;
        let mut had_quote = false;
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() && !quoted {
                break;
            }
            chars.next();
            match ch {
                '"' => {
                    quoted = !quoted;
                    had_quote = true;
                }
                '\\' if quoted && chars.peek().is_some_and(|c| matches!(c, '\\' | '"')) => {
                    if let Some(ch) = chars.next() {
                        word.push(ch);
                    }
                }
                _ => word.push(ch),
            }
        }
        words.push((word, had_quote));
    }
    words
}

#[derive(Resource, Debug, Default)]
pub struct ConsoleQueue {
    pending: Vec<ConsoleCommand>,
}

impl ConsoleQueue {
    pub fn push_line(&mut self, line: &str) {
        self.pending.extend(ConsoleCommand::parse_script(line));
    }

    pub fn drain(&mut self) -> Vec<ConsoleCommand> {
        core::mem::take(&mut self.pending)
    }
}
