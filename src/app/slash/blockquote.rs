use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "blockquote",
    description: "Quote prefix",
    action: SlashAction::PrependText("> "),
};
