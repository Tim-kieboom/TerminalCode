#[cfg(test)]
#[path = "tests/action_tests.rs"]
mod tests;

macro_rules! action_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $variant:ident => $desc:expr,
            )*
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $(
                $variant,
            )*
        }

        impl $name {
            pub fn description(&self) -> &'static str {
                match self {
                    $(
                        Self::$variant => $desc,
                    )*
                }
            }

            pub fn all() -> &'static [$name] {
                &[
                    $(Self::$variant, )*
                ]
            }

            pub fn from_description(string: &str) -> Option<Self> {
                $(
                    if string.eq_ignore_ascii_case($desc) {
                        return Some(Self::$variant);
                    }
                )*
                None
            }

            pub fn from_name(string: &str) -> Option<Self> {
                $(
                    if string.eq_ignore_ascii_case(stringify!($variant)) {
                        return Some(Self::$variant);
                    }
                )*
                None
            }
        }
    };
}

action_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum Action {
        Test => "Test",
        Quit => "Quit",
        Close => "Close",
        PrevTab => "Prev tab",
        NextTab => "Next tab",
        OpenFile => "OpenFile",
        ShowKeyBinds => "Show KeyBinds",
        ToggleBottom => "Toggle Bottom",
        SwitchBottom => "Switch Bottom",
        ToggleSidebar => "Toggle Sidebar",
        SwitchSidebar => "Switch Sidebar",
        ToggleDebugWindow => "Toggle DebugWindow",
        FocusNextPanel => "Focus Next Panel",

        ScrollUp => "Scroll up",
        ScrollTop => "Scroll top",
        ScrollDown => "Scroll down",
        ScrollLeft => "Scroll left",
        ScrollRight => "Scroll right",
        ScrollWordLeft => "Scroll word left",
        ScrollWordRight => "Scroll word right",
        ScrollBottom => "Scroll bottom",
        ScrollPageUp => "Scroll page up",
        ScrollPageDown => "Scroll page down",

        Save => "Save",
        Delete => "Delete",
        InsertTab => "Tab",
        Backspace => "Backspace",
        InsertNewline => "Enter",
        InsertChar => "Insert char",
    }
}

action_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum PanelContext {
        Editor => "editor",
        SideBar => "sidebar",
        Keybinds => "keybinds",
        BottomBar => "bottombar",
        DebugWindow => "debugWindow",
    }
}
