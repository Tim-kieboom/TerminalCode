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
        }
    };

    (@desc $variant:ident => $desc:expr) => { $desc };
    (@desc $variant:ident) => { stringify!($variant) };
}

action_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    pub enum Action {
        Quit => "Quit",
        ShowKeyBinds => "Show KeyBinds",
        ToggleSidebar => "Toggle Sidebar",
        FocusNextPanel => "Focus Next Panel",
    }
}
