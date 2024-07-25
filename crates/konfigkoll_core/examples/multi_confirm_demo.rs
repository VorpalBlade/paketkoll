use console::Style;

use konfigkoll_core::confirm::MultiOptionConfirm;

fn main() -> anyhow::Result<()> {
    let mut builder = MultiOptionConfirm::builder();
    builder
        .prompt("Are you sure?")
        .option('y', "Yes")
        .option('n', "No")
        .option('d', "show Diff")
        .prompt_style(Style::new().green())
        .options_style(Style::new().cyan())
        .default_option_style(Style::new().cyan().underlined())
        .default('N');
    let confirm = builder.build();
    let result = confirm.prompt()?;
    dbg!(result);
    Ok(())
}
