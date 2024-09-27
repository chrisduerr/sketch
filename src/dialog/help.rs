use crate::dialog::Dialog;

/// Keybinding/usage help dialog.
#[derive(Default, PartialEq, Eq)]
pub struct HelpDialog;

impl HelpDialog {
    /// Create a new help dialog.
    pub fn new() -> Self {
        Self
    }
}

impl Dialog for HelpDialog {
    fn lines(&self) -> Vec<String> {
        vec![
            String::from("MOUSE WHEEL        Change \x1b[32mbrush size\x1b[39m"),
            String::from("CTRL + LMB         Start \x1b[32mbox drawing\x1b[39m"),
            String::from("CTRL + DRAG LMB    Assisted \x1b[32mline drawing\x1b[39m"),
            String::from("CTRL + G           Open brush \x1b[32mgrapheme\x1b[39m dialog"),
            String::from("CTRL + F           Open \x1b[32mforeground color\x1b[39m dialog"),
            String::from("CTRL + B           Open \x1b[32mbackground color\x1b[39m dialog"),
            String::from("CTRL + E           \x1b[32mFill\x1b[39m at brush position"),
            String::from("CTRL + T           Toggle through \x1b[32mtext styles\x1b[39m"),
            String::from("CTRL + S           Open \x1b[32msave\x1b[39m dialog"),
            String::from("CTRL + U           \x1b[32mUndo\x1b[39m last action"),
            String::from("CTRL + R           \x1b[32mRedo\x1b[39m last undone action"),
            String::from("CTRL + L           \x1b[32mReset\x1b[39m the canvas"),
            String::from("CTRL + C           \x1b[32mExit\x1b[39m"),
        ]
    }
}
