//! Allows asking for confirmation in the CLI

use ahash::AHashMap;
use ahash::AHashSet;
use compact_str::CompactString;
use compact_str::ToCompactString;
use console::Key;
use console::Style;
use console::Term;
use itertools::Itertools;
use std::io::Write;

/// Trait to be implemented for enums that represent user prompt choices.
pub trait Choices: Copy + PartialEq + Eq {
    /// Enumerates all possible choices
    ///
    /// The tuple is (trigger character, user text, enum variant)
    fn options() -> &'static [(char, &'static str, Self)];

    /// Get the default choice (if any)
    #[must_use]
    fn default() -> Option<Self> {
        None
    }
}

/// A simple multiple choice prompt. Will look something like:
///
/// ```text
/// Are you sure? [Yes/No/show Diff]
/// ```
///
/// Letters that trigger:
/// * Must be unique
/// * Must be available as a unique code point in both upper and lower case.
/// * The convention is to put the trigger letter in uppercase in the string for
///   the option.
#[derive(Debug, Clone)]
pub struct MultiOptionConfirm<T: Choices> {
    prompt: CompactString,
    default: Option<char>,
    options: AHashMap<char, T>,
}

impl<T: Choices + 'static> MultiOptionConfirm<T> {
    /// Create a builder for this type
    #[must_use]
    pub fn builder() -> MultiOptionConfirmBuilder<T> {
        MultiOptionConfirmBuilder::new()
    }
}

/// Inner function to reduce monomorphization
fn inner_prompt(
    term: &mut Term,
    prompt: &CompactString,
    default: Option<char>,
) -> eyre::Result<char> {
    loop {
        term.write_all(prompt.as_bytes())?;
        let key = term.read_key()?;
        match key {
            Key::Char(c) => term.write_line(format!("{c}").as_str())?,
            _ => term.write_line("")?,
        }

        match key {
            Key::Enter => {
                if let Some(default) = default {
                    return Ok(default);
                }
                term.write_line("Please select an option (this prompt has no default)")?;
            }
            Key::Char(c) => return Ok(c),
            Key::Escape => {
                term.write_line("Aborted")?;
                eyre::bail!("User aborted with Escape");
            }
            Key::CtrlC => {
                term.write_line("Aborted")?;
                eyre::bail!("User aborted with Ctrl-C");
            }
            _ => {
                term.write_line("Unknown input, try again")?;
            }
        }
    }
}

impl<T: Choices> MultiOptionConfirm<T> {
    /// Run the prompt and return the user choice
    pub fn prompt(&self) -> eyre::Result<T> {
        loop {
            let mut term = Term::stdout();
            let ch = inner_prompt(&mut term, &self.prompt, self.default)?;
            let lower_case: AHashSet<_> = ch.to_lowercase().collect();
            let found = AHashSet::from_iter(self.options.keys().copied())
                .intersection(&lower_case)
                .copied()
                .collect_vec();
            if found.len() == 1 {
                return Ok(self.options[&ch]);
            }
            term.write_line("Invalid option, try again")?;
        }
    }
}

/// Inner builder for [`MultiOptionConfirm`] to reduce monomorphization bloat.
#[derive(Debug, Clone)]
struct InnerBuilder {
    prompt: Option<CompactString>,
    prompt_style: Style,
    options_style: Style,
    default_option_style: Style,
}

impl Default for InnerBuilder {
    fn default() -> Self {
        Self {
            prompt: None,
            prompt_style: Style::new().green(),
            options_style: Style::new().cyan(),
            default_option_style: Style::new().cyan().bold(),
        }
    }
}

impl InnerBuilder {
    fn render_prompt(
        &self,
        default: Option<char>,
        options: &mut dyn Iterator<Item = (char, &'static str)>,
    ) -> CompactString {
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
        let formatted = options.map(|(key, description)| {
            if Some(key) == default {
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
}

/// Builder for [`MultiOptionConfirm`].
///
/// Use [`MultiOptionConfirm::builder()`] to create a new instance.
///
/// The default style uses colours and highlights the default option with bold.
#[derive(Debug, Clone)]
pub struct MultiOptionConfirmBuilder<T: Choices> {
    inner: InnerBuilder,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Choices + 'static> MultiOptionConfirmBuilder<T> {
    fn new() -> Self {
        Self {
            inner: InnerBuilder::default(),
            _phantom: Default::default(),
        }
    }

    /// Set prompt to use. Required.
    pub fn prompt(&mut self, prompt: &str) -> &mut Self {
        self.inner.prompt = Some(prompt.to_compact_string());
        self
    }

    /// Set style for question part of the prompt.
    pub fn prompt_style(&mut self, style: Style) -> &mut Self {
        self.inner.prompt_style = style;
        self
    }

    /// Set style for the options.
    pub fn options_style(&mut self, style: Style) -> &mut Self {
        self.inner.options_style = style;
        self
    }

    /// Set style for the default option.
    pub fn default_option_style(&mut self, style: Style) -> &mut Self {
        self.inner.default_option_style = style;
        self
    }

    #[must_use]
    pub fn build(&self) -> MultiOptionConfirm<T> {
        assert!(T::options().len() >= 2, "At least two options are required");
        let mut default_char = None;
        let default = T::default();
        let options: AHashMap<char, T> = T::options()
            .iter()
            .inspect(|(key, _, val)| {
                // Using inspect for side effects, not sure if it is overly clever or just plain
                // stupid. But it means we only have to iterate over the options once.
                if Some(*val) == default {
                    default_char = Some(*key);
                }
                assert!(key.is_lowercase());
            })
            .map(|(key, _, val)| (*key, *val))
            .collect();
        let prompt = self.inner.render_prompt(
            default_char,
            &mut T::options().iter().map(|(key, desc, _)| (*key, *desc)),
        );
        MultiOptionConfirm {
            prompt,
            default: default_char,
            options,
        }
    }
}
