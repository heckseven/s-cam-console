/// The S-CAM mark, as it appears on the badge's startup screen.
///
/// Generated from scam-splash.svg at its own 2px grid, so it is the same drawing the badge
/// shows rather than a second version of it that can drift.
///
/// Drawn with half-block characters, two source rows per line. A terminal cell is about
/// twice as tall as it is wide, so one character per pixel renders the mark stretched;
/// pairing the rows this way keeps it roughly square and halves the height.
pub const SCAM_BANNER: &[&str] = &[
    "  ▄▀▀▄           ▄▀▀▄",
    "▄▀▀  █    ▄▀▄    █  ▀▀▄",
    "▀▄▄▄  ▀ ▄▀   ▀▄ ▀  ▄▄▄▀",
    "    ▀▄ █   █   █ ▄▀",
    "       █   █   █   ▄▀▀▀▄   ▄▀▄   █   █",
    "        ▀▄  ▀▄▀    █      █   █  █▀▄▀█",
    "       ▄▀ ▀▄  ▀▄   █   ▄  █▀▀▀█  █ ▀ █",
    "     ▄ █   █   █ ▄  ▀▀▀   ▀   ▀  ▀   ▀",
    " ▄▄▄▀  ▀▄  ▀  ▄▀  ▀▄▄▄",
    "█    ▄▀  ▀▄ ▄▀  ▀▄    █",
    " ▀█  █     ▀     █  █▀",
    "   ▀▀             ▀▀",
];

/// Print the mark.
///
/// Lives here rather than in the shell because the help command prints it too, and two
/// copies of a drawing is one copy too many.
pub fn print_banner() {
    println!("");
    for row in SCAM_BANNER {
        println!("{}", row);
    }
    println!("");
}
