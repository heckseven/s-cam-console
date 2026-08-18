use bao1x_api::*;
use xous::msg_scalar_unpack;





pub fn start_shell() {
    std::thread::spawn(move || {
        shell();
    });
}

////////////////// local message passing from Ux Callback
use num_traits::*;


#[derive(Debug, num_derive::FromPrimitive, num_derive::ToPrimitive)]
enum ConsoleOp {
    /// A character is incoming
    Keypress,
    /// exit the application
    Quit,
}

pub(crate) const SERVER_NAME_SHELLCHAT: &str = "_Bao console application_"; // used internally by xous-names

fn shell() {
    let xns = xous_names::XousNames::new().unwrap();
    // unlimited connections allowed, this is a user app and it's up to the app to decide its policy
    let shch_sid = xns.register_name(SERVER_NAME_SHELLCHAT, None).expect("can't register server");

    let kbd = keyboard::Keyboard::new(&xns).unwrap();

    let mut repl = crate::repl::Repl::new(&xns);
    let mut update_repl = false;
    let mut was_callback = false;

    // register this late because the REPL can take a while to init as it depends on the PDDB.
    kbd.register_listener(SERVER_NAME_SHELLCHAT, ConsoleOp::Keypress.to_u32().unwrap() as usize);
    let mut input = String::new();
    loop {
        let msg = xous::receive_message(shch_sid).unwrap();
        let console_op: Option<ConsoleOp> = FromPrimitive::from_usize(msg.body.id());
        log::debug!("{:?}", console_op);
        match console_op {
            Some(ConsoleOp::Keypress) => msg_scalar_unpack!(msg, k1, _k2, _k3, _k4, {
                let k = char::from_u32(k1 as u32).unwrap_or('\u{0000}');
                if k1 == 0x08 {
                    // backspace character
                    input.pop(); // returns None if empty
                } else if matches!(k, '\u{1f525}' | '\u{23f0}' | '\u{1f53c}' | '\u{1f53d}'
                                    | '\u{23ef}' | '\u{2190}' | '\u{2192}' | '\u{2191}'
                                    | '\u{2193}' | '\u{2234}')
                {
                    // Badge controls, not typing. This listener receives the physical buttons
                    // as well as anything arriving over serial, so walking the badge's menus
                    // was pushing the middle button, the jog press and the arrows straight
                    // into the command line and echoing them back.
                    //
                    // The first four can only come from the hardware - the centre button, the
                    // RTC wakeup and the two orientation events. The last three are shared
                    // with a terminal, which sends them as escape sequences that the keyboard
                    // server translates to these same characters, but neither source wants
                    // them inserted: there is no line editing here (see the history branch
                    // above), so an arrow has nothing useful to do and only ever arrived as a
                    // literal character in the middle of a command.
                    //
                    // Up and down are here too. They used to run a half-built history recall:
                    // the branch that clears the current line sat behind `if false`, so it
                    // printed the recalled command on a new line and silently threw away
                    // whatever had been typed. Rotating the jog wheel did that from across
                    // the room. Real history needs line editing first, which needs the print
                    // path to stop buffering until newline - Repl::get_history is still there
                    // for whoever does that.
                } else if k != '\u{0000}' && k != '\n' && k != '\r' {
                    input.push(k);
                    // Echo as it is typed. Output is buffered until a newline, so this needs
                    // an explicit flush - without one you type blind until you press Enter.
                    print!("{}", k);
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                } else if k == '\n' || k == '\r' {
                    println!("");
                    if input.is_empty() {
                        // Enter on an empty line shows the same help as `help` does. The
                        // badge cannot tell when a terminal attaches, so there is no "on
                        // connect" moment to hook - this is the gesture that stands in for it.
                        repl.input("help").expect("REPL crashed");
                        update_repl = true;
                        was_callback = false;
                    } else {
                        repl.input(input.as_str()).expect("REPL crashed");
                        update_repl = true;
                        was_callback = false;
                    }
                    input.clear();
                }
            }),
            Some(ConsoleOp::Quit) => {
                log::error!("got Quit");
                break;
            }
            _ => {
                log::trace!("got unknown message, treating as callback");
                repl.msg(msg);
                update_repl = true;
                was_callback = true;
            }
        }
        if update_repl {
            repl.update(was_callback, true).expect("REPL had problems updating");
            update_repl = false;
        }
    }

    // clean up our program
    log::error!("shell loop exit, destroying servers");
    xns.unregister_server(shch_sid).unwrap();
    xous::destroy_server(shch_sid).unwrap();
}
