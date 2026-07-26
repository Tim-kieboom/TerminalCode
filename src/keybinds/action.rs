macro_rules! action_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $variant:ident $(=> $desc:expr)?,
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
                        Self::$variant => action_enum!(@desc $variant $(=> $desc)?),
                    )*
                }
            }

            pub fn all() -> &'static [$name] {
                &[
                    $(Self::$variant, )*
                ]
            }

            pub fn from_str(string: &str) -> Option<Self> {
                match string.to_lowercase().as_str() {
                    $(
                        action_enum!(@desc $variant $(=> $desc)?) => Some(Self::$variant),
                    )*
                    _ => None,
                }
            }
        }
    };

    (@desc $variant:ident => $desc:expr) => { $desc };
    (@desc $variant:ident) => { stringify!($variant) };
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
        FocusNextPanel => "Focus Next Panel",
        ScrollUp => "Scroll up",
        ScrollDown => "Scroll down",
        ScrollPageUp => "Scroll page up",
        ScrollPageDown => "Scroll page down",
        ScrollTop => "Scroll top",
        ScrollBottom => "Scroll bottom",
    }
}

action_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum PanelContext {
        SideBar => "sidebar",
        Editor => "editor",
        Keybinds => "keybinds",
        BottomBar => "bottombar",
    }
}
