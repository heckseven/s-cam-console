// photo.rs — REPL commands for pulling photos off the badge over serial.
//
// The photos live in dc34-vault's PDDB, not here, so these are thin: they ask the vault by
// opcode and it writes the image out of the same CDC port this command arrived on.
//
//   photo list        how many photos are stored
//   photo get <n>     photo n as a base64 BMP data URI
//   photo ascii <n>   photo n as ASCII art
//
// Indices are zero-based and match the order of the photos list on the badge.
//
// The opcode numbers are literals because dc34-console cannot see dc34-vault's enum. They are
// pinned on the other side by compile-time asserts in vault_api.rs, next to a note pointing
// back here - the same arrangement the `image` command already uses.

use core::fmt::Write;

use String;

use crate::{CommonEnv, ShellCmdApi};

/// dc34-vault's server name, as registered with xous-names.
const VAULT_SERVER: &str = "_Vault2_";
/// VaultOp::SerialPhotoCount
const OP_PHOTO_COUNT: usize = 1049;
/// VaultOp::SerialPhotoGet
const OP_PHOTO_GET: usize = 1050;

#[derive(Debug)]
pub struct Photo {}
impl Photo {
    pub fn new() -> Self { Photo {} }
}

impl<'a> ShellCmdApi<'a> for Photo {
    cmd_api!(photo);

    fn process(&mut self, args: String, env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        let mut ret = String::new();
        let helpstring = "photo [list | get <n> | ascii <n>]";

        let mut tokens = args.split_whitespace();
        let conn = match env.xns.request_connection_blocking(VAULT_SERVER) {
            Ok(c) => c,
            Err(_) => {
                write!(ret, "ERR vault not running").unwrap();
                return Ok(Some(ret));
            }
        };

        match tokens.next() {
            Some("list") => match xous::send_message(
                conn,
                xous::Message::new_blocking_scalar(OP_PHOTO_COUNT, 0, 0, 0, 0),
            ) {
                Ok(xous::Result::Scalar1(n)) => {
                    write!(ret, "{} photo(s), indices 0..{}", n, n.saturating_sub(1)).unwrap()
                }
                _ => write!(ret, "ERR no answer from vault").unwrap(),
            },
            Some(verb @ ("get" | "ascii")) => {
                let index = match tokens.next().map(|t| t.parse::<usize>()) {
                    Some(Ok(i)) => i,
                    _ => {
                        write!(ret, "ERR need an index: photo {} <n>", verb).unwrap();
                        return Ok(Some(ret));
                    }
                };
                let as_art = if verb == "ascii" { 1 } else { 0 };
                // Blocking: the image is written to this same port, so returning before it is
                // out would interleave the reply with the picture.
                match xous::send_message(
                    conn,
                    xous::Message::new_blocking_scalar(OP_PHOTO_GET, index, as_art, 0, 0),
                ) {
                    Ok(xous::Result::Scalar1(1)) => write!(ret, "SUCCESS").unwrap(),
                    Ok(xous::Result::Scalar1(_)) => {
                        write!(ret, "ERR no photo {}", index).unwrap()
                    }
                    _ => write!(ret, "ERR no answer from vault").unwrap(),
                }
            }
            _ => write!(ret, "{}", helpstring).unwrap(),
        }
        Ok(Some(ret))
    }
}
