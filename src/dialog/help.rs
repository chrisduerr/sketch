use crate::dialog::Dialog;

/// Keybinding/usage help dialog.
#[derive(Default, PartialEq, Eq)]
pub struct HelpDialog;

impl HelpDialog {
    /// Create a new help dialog.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Dialog for HelpDialog {
    fn lines(&self) -> Vec<String> {
        vec![
            String::from("MOUSE WHEEL        Change brush size"),
            String::from("CTRL + LMB         Start box drawing"),
            String::from("CTRL + DRAG LMB    Assisted line drawing"),
            String::from("CTRL + T           Open brush character dialog"),
            String::from("CTRL + F           Open foreground color dialog"),
            String::from("CTRL + B           Open background color dialog"),
            String::from("CTRL + U           Undo last action"),
            String::from("CTRL + R           Redo last undone action"),
            String::from("CTRL + L           Reset the canvas"),
            String::from("CTRL + C           Exit"),
        ]
    }
}
