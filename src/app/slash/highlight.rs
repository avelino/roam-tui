use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "highlight",
    description: "Highlight text",
    action: SlashAction::InsertPair {
        open: "^^",
        close: "^^",
    },
};
