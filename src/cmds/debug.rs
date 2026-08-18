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

pub struct Debug {
    pddb: Pddb,
}
impl Debug {
    pub fn new() -> Self { Debug { pddb: Pddb::new() } }
}

impl<'a> ShellCmdApi<'a> for Debug {
    cmd_api!(debug);

    fn help(&self) -> &'static str { "print badge state: storage, uptime, connections" }

    fn process(&mut self, _args: String, env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        let mut ret = String::new();

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

        write!(ret, "log level   set with `debug` on the badge's own log stream").unwrap();
        Ok(Some(ret))
    }
}
