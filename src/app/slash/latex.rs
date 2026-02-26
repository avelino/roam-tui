use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "latex",
    description: "LaTeX formula",
    action: SlashAction::InsertPair {
        open: "$$",
        close: "$$",
    },
};
