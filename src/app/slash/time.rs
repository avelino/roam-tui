use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "time",
    description: "Insert current time",
    action: SlashAction::InsertTime,
};
