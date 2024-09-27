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
            String::from("MOUSE WHEEL        \x1b[32mbrush size\x1b[39m change"),
            String::from("CTRL + LMB         \x1b[32mbox drawing\x1b[39m mode"),
            String::from("CTRL + DRAG LMB    \x1b[32mline drawing\x1b[39m mode"),
            String::from("CTRL + G           \x1b[32mgrapheme\x1b[39m picker"),
            String::from("CTRL + F           \x1b[32mforeground color\x1b[39m picker"),
            String::from("CTRL + B           \x1b[32mbackground color\x1b[39m picker"),
            String::from("CTRL + E           \x1b[32mfill\x1b[39m at brush position"),
            String::from("CTRL + T           \x1b[32mtext styles\x1b[39m toggle"),
            String::from("CTRL + S           \x1b[32msave\x1b[39m sketch"),
            String::from("CTRL + O           \x1b[32mopen\x1b[39m existing sketch"),
            String::from("CTRL + U           \x1b[32mundo\x1b[39m last action"),
            String::from("CTRL + R           \x1b[32mredo\x1b[39m last undone action"),
            String::from("CTRL + L           \x1b[32mreset\x1b[39m the canvas"),
            String::from("CTRL + C           \x1b[32mexit\x1b[39m"),
            String::from("ESC                \x1b[32mclose\x1b[39m dialog"),
        ]
    }
}
