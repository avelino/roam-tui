use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "h3",
    description: "Heading 3",
    action: SlashAction::PrependText("### "),
};
