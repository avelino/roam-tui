use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "bold",
    description: "Bold text",
    action: SlashAction::InsertPair {
        open: "**",
        close: "**",
    },
};
