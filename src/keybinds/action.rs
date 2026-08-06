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
                let string = string.to_lowercase();
                match string.as_str() {
                    $(
                        $desc => Some(Self::$variant),
                    )*
                    _ => None,
                }
            }

            pub fn from_name(string: &str) -> Option<Self> {
                match string {
                    $(
                        stringify!($variant) => Some(Self::$variant),
                    )*
                    _ => None,
                }
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
        OpenFile => "OpenFile",
        ShowKeyBinds => "Show KeyBinds",
        ToggleBottom => "Toggle Bottom",
        ToggleSidebar => "Toggle Sidebar",
        ToggleDebugWindow => "Toggle DebugWindow",
        FocusNextPanel => "Focus Next Panel",

        ScrollUp => "Scroll up",
        ScrollDown => "Scroll down",
        ScrollLeft => "Scroll left",
        ScrollRight => "Scroll right",
        ScrollPageUp => "Scroll page up",
        ScrollPageDown => "Scroll page down",
        ScrollTop => "Scroll top",
        ScrollBottom => "Scroll bottom",

        InsertChar => "Insert char",
        InsertNewline => "Enter",
        Backspace => "Backspace",
        InsertTab => "Tab",
        Delete => "Delete",
        Save => "Save",
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
