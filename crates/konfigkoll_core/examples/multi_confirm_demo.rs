use console::Style;
use konfigkoll_core::confirm::Choices;
use konfigkoll_core::confirm::MultiOptionConfirm;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PromptChoices {
    Yes,
    No,
    ShowDiff,
}

impl Choices for PromptChoices {
    fn options() -> &'static [(char, &'static str, Self)] {
        &[
            ('y', "Yes", Self::Yes),
            ('n', "No", Self::No),
            ('d', "show Diff", Self::ShowDiff),
        ]
    }

    fn default() -> Option<Self> {
        Some(Self::No)
    }
}

fn main() -> eyre::Result<()> {
    let mut builder = MultiOptionConfirm::builder();
    builder
        .prompt("Are you sure?")
        .prompt_style(Style::new().green())
        .options_style(Style::new().cyan())
        .default_option_style(Style::new().cyan().underlined());
    let confirm: MultiOptionConfirm<PromptChoices> = builder.build();
    let result = confirm.prompt()?;
    dbg!(result);
    Ok(())
}
