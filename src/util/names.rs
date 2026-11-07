//! Fieldless enums that are spelled out as text somewhere (a config value, a
//! JSON field, a command-line word): one list of `Variant => "text"` gives
//! the enum, its spelling, its parse and the list of all its variants.

macro_rules! named_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            pub fn parse(name: &str) -> Option<$name> {
                Self::ALL.iter().copied().find(|value| value.as_str() == name)
            }
        }
    };
}
