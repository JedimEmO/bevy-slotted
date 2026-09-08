//! Game menus for slotted (menus M2 contract, `docs/design/menus-m2-contract.md`).
//!
//! A main menu, a pause screen, settings with persistence, confirm dialogs,
//! toasts, text pages, a hint bar and a dialogue runner (menus M3,
//! `docs/design/menus-m3-contract.md`), shipped as screen templates a game
//! inherits from and a mod injects into. The crate is a consumer of the
//! `slotted-ui` extension API: templates are data, every string has an
//! English fallback, and the crate speaks to the game through messages.

#![forbid(unsafe_code)]

pub mod confirm;
pub mod dialogue;
pub mod dialogue_screen;
pub mod hint_bar;
pub mod page;
pub mod plugin;
pub mod settings;
pub mod strings;
pub mod templates;
pub mod toast;

pub use confirm::{ConfirmResult, ConfirmSpec, PendingConfirm, confirm};
pub use dialogue::{
    ActiveDialogue, ChoiceOption, Condition, Dialogue, DialogueAssets, DialogueChoice,
    DialogueConfig, DialogueEnded, DialogueError, DialogueId, DialogueLoader, DialogueNode,
    DialogueNodeEntered, Dialogues, EndReason, HistoryLine, NodeId, advance_dialogue,
    choose_dialogue, end_dialogue, jump_dialogue, start_dialogue, start_dialogue_with,
};
pub use dialogue_screen::{
    DialogueChoices, DialogueLine, DialogueOption, DialogueScreen, OPTION_TAG, open_history,
};
pub use hint_bar::{HintBar, HintEntries, HintEntry};
pub use page::{PageSpec, open_page};
pub use plugin::{MenuChoice, MenuConfig, MenuPlugin};
pub use settings::{
    FileSettings, MemorySettings, SavedSettings, Settings, SettingsReset, SettingsRow,
    SettingsSpec, SettingsStorage, SettingsStore, SettingsTab,
};
pub use templates::kinds;
pub use toast::{Toast, ToastLevel, ToastSpec, Toasts, toast};

/// The common names.
pub mod prelude {
    pub use crate::{
        ActiveDialogue, ConfirmResult, ConfirmSpec, Dialogue, DialogueChoice, DialogueConfig,
        DialogueEnded, DialogueId, DialogueNodeEntered, Dialogues, MenuChoice, MenuConfig,
        MenuPlugin, PageSpec, SavedSettings, Settings, SettingsRow, SettingsSpec, SettingsStorage,
        SettingsTab, ToastLevel, ToastSpec, confirm, open_page, start_dialogue, toast,
    };
}
