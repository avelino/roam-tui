use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "done",
    description: "Add DONE checkbox",
    action: SlashAction::PrependText("{{[[DONE]]}} "),
};
