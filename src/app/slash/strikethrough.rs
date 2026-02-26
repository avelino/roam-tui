use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "strikethrough",
    description: "Strikethrough text",
    action: SlashAction::InsertPair {
        open: "~~",
        close: "~~",
    },
};
