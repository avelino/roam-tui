use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "todo",
    description: "Add TODO checkbox",
    action: SlashAction::PrependText("{{[[TODO]]}} "),
};
