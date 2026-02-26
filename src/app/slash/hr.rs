use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "hr",
    description: "Horizontal rule",
    action: SlashAction::InsertText("---"),
};
