pub mod config;
pub mod phrases;
pub mod shell;

/// Names the per-user registry key and data folder. Changing it orphans the
/// settings and learned phrases stored under the previous name.
pub const PRODUCT_NAME: &str = "InputMethodEditor";

/// The settings app, in the architecture folder beside chewing_tip.dll.
pub const SETTINGS_EXE: &str = "InputMethodEditorSettings.exe";

/// Opens the settings app through the shell rather than as our child: the
/// IME runs inside other apps, and a Store app or a packaged one like Notepad
/// would start it in its own container, where the settings it saves may not
/// reach the registry the IME reads.
pub const SETTINGS_SCHEME: &str = "inputmethodeditor-settings";
