use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "code",
    description: "Insert code block",
    action: SlashAction::InsertCodeBlock,
};
