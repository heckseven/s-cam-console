// debug.rs — one place to ask the badge what state it is in.
//
// Written because diagnosing this badge has repeatedly meant reflashing it with a log line
// added. Anything worth printing during an investigation belongs here instead, where it can
// be read from a terminal without a build.

use core::fmt::Write;

use String;
use pddb::Pddb;

use crate::{CommonEnv, ShellCmdApi};

const PHOTOS_DICT: &str = "vault.photos";
const BOOKMARKS_DICT: &str = "vault.bookmarks";
const COUNTER_KEY: &str = "__counter__";
const VAULT_SERVER: &str = "_Vault2_";
/// VaultOp::SetLogLevel. Pinned in dc34-vault/src/vault_api.rs with a compile-time assert.
const OP_SET_LOG_LEVEL: u32 = 1052;

pub struct Debug {
    pddb: Pddb,
}
impl Debug {
    pub fn new() -> Self { Debug { pddb: Pddb::new() } }
}

impl<'a> ShellCmdApi<'a> for Debug {
    cmd_api!(debug);

    fn help(&self) -> &'static str { "print badge state; `debug log on|off` for verbose logging" }

    fn usage(&self) -> &'static str {
        "debug             storage, uptime and whether the vault is answering\n\
         debug log on      verbose logging from the console and the vault\n\
         debug log off     quiet again (the default)"
    }

    fn process(&mut self, args: String, env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        let mut ret = String::new();

        // Both processes log onto the same interface this REPL is speaking over, so verbose
        // logging is off unless it is asked for. Walking the badge's menus alone emits a
        // burst of lines from ux_api::menu, which is linked into the vault.
        let mut tokens = args.split_whitespace();
        if tokens.next() == Some("log") {
            let verbose = match tokens.next() {
                Some("on") => true,
                Some("off") => false,
                other => {
                    write!(ret, "ERR say `debug log on` or `debug log off`, not {:?}", other).unwrap();
                    return Ok(Some(ret));
                }
            };
            log::set_max_level(if verbose {
                log::LevelFilter::Info
            } else {
                log::LevelFilter::Warn
            });
            match env.xns.request_connection_blocking(VAULT_SERVER) {
                Ok(conn) => {
                    xous::send_message(
                        conn,
                        xous::Message::new_scalar(
                            OP_SET_LOG_LEVEL as usize,
                            if verbose { 1 } else { 0 },
                            0,
                            0,
                            0,
                        ),
                    )
                    .ok();
                    write!(ret, "logging {} for the console and the vault",
                           if verbose { "verbose" } else { "quiet" }).unwrap();
                }
                // The console's own level still changed, so say what actually happened.
                Err(_) => write!(ret, "console logging {}; vault not running",
                                 if verbose { "verbose" } else { "quiet" }).unwrap(),
            }
            return Ok(Some(ret));
        }

        let up = env.ticktimer.elapsed_ms();
        writeln!(ret, "uptime      {}h {:02}m {:02}s", up / 3_600_000, (up / 60_000) % 60, (up / 1000) % 60)
            .unwrap();

        let photos = self.pddb.list_keys(PHOTOS_DICT, None).unwrap_or_default();
        writeln!(ret, "photos      {} stored", photos.len()).unwrap();

        let mut qrs = self.pddb.list_keys(BOOKMARKS_DICT, None).unwrap_or_default();
        qrs.retain(|k| k != COUNTER_KEY);
        writeln!(ret, "qr codes    {} stored", qrs.len()).unwrap();

        match self.pddb.list_dict(None) {
            Ok(dicts) => writeln!(ret, "pddb dicts  {}", dicts.join(", ")).unwrap(),
            Err(e) => writeln!(ret, "pddb dicts  unreadable: {:?}", e).unwrap(),
        }

        // Whether the vault is answering at all. A badge whose UI process has died still has
        // a working console, which is exactly the situation worth being able to detect.
        match env.xns.request_connection("_Vault2_") {
            Ok(_) => writeln!(ret, "vault       responding").unwrap(),
            Err(e) => writeln!(ret, "vault       NOT RESPONDING: {:?}", e).unwrap(),
        }

        writeln!(ret, "log level   {} (`debug log on` for verbose)", log::max_level()).unwrap();
        write!(ret, "console     responding").unwrap();
        Ok(Some(ret))
    }
}
