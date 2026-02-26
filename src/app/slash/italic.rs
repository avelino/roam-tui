use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "italic",
    description: "Italic text",
    action: SlashAction::InsertPair {
        open: "__",
        close: "__",
    },
};
