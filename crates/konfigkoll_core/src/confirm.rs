//! Allows asking for confirmation in the CLI

use std::io::Write;

use ahash::AHashSet;
use compact_str::CompactString;
use compact_str::ToCompactString;
use console::Key;
use console::Style;
use console::Term;
use itertools::Itertools;

/// A simple multiple choice prompt. Will look something like:
///
/// ```text
/// Are you sure? [Yes/No/show Diff]
/// ```
///
/// Letters that trigger:
/// * Must be unique
/// * Must be available as a unique code point in both upper and lower case.
/// * The convention is to put the trigger letter in uppercase in the string for the option.
#[derive(Debug, Clone)]
pub struct MultiOptionConfirm {
    prompt: CompactString,
    default: Option<char>,
    options: AHashSet<char>,
}

impl MultiOptionConfirm {
    /// Create a builder for this type
    pub fn builder() -> MultiOptionConfirmBuilder {
        MultiOptionConfirmBuilder::new()
    }

    /// Run the prompt and return the user choice
    pub fn prompt(&self) -> anyhow::Result<char> {
        let mut term = Term::stdout();
        loop {
            term.write_all(self.prompt.as_bytes())?;
            let key = term.read_key()?;
            match key {
                Key::Char(c) => term.write_line(format!("{c}").as_str())?,
                _ => term.write_line("")?,
            }

            match key {
                Key::Enter => {
                    if let Some(default) = self.default {
                        return Ok(default);
                    } else {
                        term.write_line("Please select an option (this prompt has no default)")?;
                    }
                }
                Key::Char(c) => {
                    let lower_case: AHashSet<_> = c.to_lowercase().collect();
                    let found = self.options.intersection(&lower_case).count() > 0;
                    if found {
                        return Ok(c);
                    } else {
                        term.write_line("Invalid option, try again")?;
                    }
                }
                Key::Escape => {
                    term.write_line("Aborted")?;
                    anyhow::bail!("User aborted with Escape");
                }
                Key::CtrlC => {
                    term.write_line("Aborted")?;
                    anyhow::bail!("User aborted with Ctrl-C");
                }
                _ => {
                    term.write_line("Unknown input, try again")?;
                }
            }
        }
    }
}

/// Builder for [`MultiOptionConfirm`].
///
/// Use [`MultiOptionConfirm::builder()`] to create a new instance.
///
/// The default style uses colours and highlights the default option with bold.
#[derive(Debug, Clone)]
pub struct MultiOptionConfirmBuilder {
    prompt: Option<CompactString>,
    default: Option<char>,
    prompt_style: Style,
    options_style: Style,
    default_option_style: Style,
    options: Vec<(char, CompactString)>,
}

impl MultiOptionConfirmBuilder {
    fn new() -> Self {
        Self {
            prompt: None,
            default: None,
            prompt_style: Style::new().green(),
            options_style: Style::new().cyan(),
            default_option_style: Style::new().cyan().bold(),
            options: Vec::new(),
        }
    }

    /// Set prompt to use. Required.
    pub fn prompt(&mut self, prompt: &str) -> &mut Self {
        self.prompt = Some(prompt.to_compact_string());
        self
    }

    /// Set default choice. Optional.
    pub fn default(&mut self, default: char) -> &mut Self {
        self.default = Some(
            default
                .to_lowercase()
                .next()
                .expect("Letter is not available as lower case"),
        );
        self
    }

    /// Add an option. At least two are required.
    pub fn option(&mut self, key: char, value: &str) -> &mut Self {
        self.options.push((
            key.to_lowercase()
                .next()
                .expect("Letter is not available as lower case"),
            value.to_compact_string(),
        ));
        self
    }

    /// Set style for question part of the prompt.
    pub fn prompt_style(&mut self, style: Style) -> &mut Self {
        self.prompt_style = style;
        self
    }

    /// Set style for the options.
    pub fn options_style(&mut self, style: Style) -> &mut Self {
        self.options_style = style;
        self
    }

    /// Set style for the default option.
    pub fn default_option_style(&mut self, style: Style) -> &mut Self {
        self.default_option_style = style;
        self
    }

    fn render_prompt(&self) -> CompactString {
        let mut prompt = self
            .prompt_style
            .apply_to(&self.prompt.as_ref().expect("A prompt must be set"))
            .to_compact_string();

        prompt.push_str(
            self.options_style
                .apply_to(" [")
                .to_compact_string()
                .as_str(),
        );
        let formatted = self.options.iter().map(|(key, description)| {
            if Some(*key) == self.default {
                self.default_option_style
                    .apply_to(description)
                    .to_compact_string()
            } else {
                self.options_style.apply_to(description).to_compact_string()
            }
        });
        let options = Itertools::intersperse(
            formatted,
            self.options_style.apply_to("/").to_compact_string(),
        )
        .collect::<String>();
        prompt.push_str(options.as_str());
        prompt.push_str(
            self.options_style
                .apply_to("] ")
                .to_compact_string()
                .as_str(),
        );
        prompt
    }

    pub fn build(&self) -> MultiOptionConfirm {
        if self.options.len() < 2 {
            panic!("At least two options are required");
        }
        MultiOptionConfirm {
            prompt: self.render_prompt(),
            default: self.default,
            options: self.options.iter().map(|(key, _)| *key).collect(),
        }
    }
}
