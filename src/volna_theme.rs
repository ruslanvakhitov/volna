//! Volna's sea-glass palette, expressed through the shared kit tokens.
use rune_kit::Kit;
use std::sync::OnceLock;

pub fn kit(dark: bool) -> &'static Kit {
    static DARK: OnceLock<Kit> = OnceLock::new();
    static LIGHT: OnceLock<Kit> = OnceLock::new();
    let build = || {
        let mut k = rune_theme_opencode::kit(dark).clone();
        let mint = |step| rune_theme_opencode::scale("mint", dark, step);
        k.c.text_accent = mint(11);
        k.c.icon_accent = mint(11);
        k.c.bg_accent = mint(4);
        k.c.border_focus = mint(9);
        k.c.surface_hover = mint(3);
        k.radius.md = 12.0;
        k
    };
    if dark {
        DARK.get_or_init(build)
    } else {
        LIGHT.get_or_init(build)
    }
}
