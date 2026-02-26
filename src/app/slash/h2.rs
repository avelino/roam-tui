use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "h2",
    description: "Heading 2",
    action: SlashAction::PrependText("## "),
};
