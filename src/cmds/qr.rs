// qr.rs — REPL access to the saved QR codes.
//
//   qr list        every saved code, numbered
//   qr get <n>     the URL of code n on its own, for piping somewhere
//
// Reads the vault's dictionary directly. That is safe for reading - the record is three
// lines, url then label then timestamp - but adding one is deliberately not done here: the
// keys come from a counter record the vault maintains, and a second implementation of that
// would be a second thing to keep in step. `qr add` belongs behind a vault opcode.

use core::fmt::Write;
use std::io::Read;

use String;
use pddb::Pddb;

use crate::{CommonEnv, ShellCmdApi};

const BOOKMARKS_DICT: &str = "vault.bookmarks";
/// The vault keeps its key counter in the same dictionary; it is not a bookmark.
const COUNTER_KEY: &str = "__counter__";

pub struct Qr {
    pddb: Pddb,
}
impl Qr {
    pub fn new() -> Self { Qr { pddb: Pddb::new() } }

    /// Saved codes as (key, url), in the order the badge shows them.
    fn codes(&mut self) -> Vec<(String, String)> {
        let mut keys = self.pddb.list_keys(BOOKMARKS_DICT, None).unwrap_or_default();
        keys.retain(|k| k != COUNTER_KEY);
        keys.sort(); // zero-padded hex, so lexical order is insertion order
        let mut out = Vec::new();
        for key in keys {
            if let Ok(mut rec) =
                self.pddb.get(BOOKMARKS_DICT, &key, None, false, false, None, None::<fn()>)
            {
                let mut data = Vec::new();
                if rec.read_to_end(&mut data).is_ok() {
                    if let Ok(body) = std::str::from_utf8(&data) {
                        let url = body.split('\n').next().unwrap_or("").to_string();
                        out.push((key, url));
                    }
                }
            }
        }
        out
    }
}

impl<'a> ShellCmdApi<'a> for Qr {
    cmd_api!(qr);

    fn help(&self) -> &'static str { "list and read saved QR codes" }

    fn usage(&self) -> &'static str {
        "qr list                every saved code, numbered\n    \
         qr get <n>             the URL of code n on its own"
    }

    fn process(&mut self, args: String, _env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        let mut ret = String::new();
        let mut tokens = args.split_whitespace();
        match tokens.next() {
            Some("list") => {
                let codes = self.codes();
                if codes.is_empty() {
                    write!(ret, "no saved QR codes").unwrap();
                } else {
                    for (n, (_, url)) in codes.iter().enumerate() {
                        writeln!(ret, "  {:>2}  {}", n, url).unwrap();
                    }
                    write!(ret, "{} code(s)", codes.len()).unwrap();
                }
            }
            Some("get") => {
                let index = match tokens.next().map(|t| t.parse::<usize>()) {
                    Some(Ok(i)) => i,
                    _ => {
                        write!(ret, "ERR need an index: qr get <n>").unwrap();
                        return Ok(Some(ret));
                    }
                };
                match self.codes().get(index) {
                    Some((_, url)) => write!(ret, "{}", url).unwrap(),
                    None => write!(ret, "ERR no code {}", index).unwrap(),
                }
            }
            _ => write!(ret, "qr [list | get <n>]").unwrap(),
        }
        Ok(Some(ret))
    }
}
