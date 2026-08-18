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
use std::io::Write as FsWrite;

use String;
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use pddb::Pddb;

use crate::{CommonEnv, ShellCmdApi};

/// dc34-vault's server name, as registered with xous-names.
const VAULT_SERVER: &str = "_Vault2_";
/// VaultOp::SerialPhotoCount
const OP_PHOTO_COUNT: usize = 1049;
/// VaultOp::SerialPhotoGet
const OP_PHOTO_GET: usize = 1050;

// -- `photo put` wire format, unchanged from the command it replaces -----------------
//
//   [0..2]   u16  chunk index, big-endian
//   [2..66]  u8*64 pixel data
//   [66..70] u32  CRC-32 of bytes [0..66], big-endian
//
// A photo is 128x128 at one bit per pixel: 2048 bytes, so 32 chunks of 64.

/// The dictionary the badge keeps camera photos in. Written directly rather than through the
/// vault, because a photo is 2KB and that does not fit in a scalar message - and the PDDB is
/// shared, so there is nothing to route around.
const VAULT_PHOTOS_DICT: &str = "vault.photos";
const PHOTO_BYTES: usize = 2048;
const PHOTO_CAP: usize = 32;

const CHUNK_DATA_SIZE: usize = 64;
const CHUNK_INDEX_BYTES: usize = 2;
const CHUNK_WIRE_SIZE: usize = CHUNK_INDEX_BYTES + CHUNK_DATA_SIZE + 4; // 70
const NUM_CHUNKS: usize = PHOTO_BYTES / CHUNK_DATA_SIZE; // 32
const BITMAP_WORDS: usize = PHOTO_BYTES / 4; // 512


/// A self-contained uploader, kept on the badge so nobody has to go and find one.
///
/// `photo script > upload_photo.py` and it is ready to run. The wire format it speaks is the
/// one implemented below, and the bit packing matches what the camera writes - verified by
/// exporting a photo, repacking it with this code, and comparing.
const UPLOAD_SCRIPT: &str = r##"#!/usr/bin/env python3
"""Send an image to an S-CAM badge over its serial console.

    python3 upload_photo.py picture.png            # needs Pillow
    python3 upload_photo.py picture.raw --raw      # 2048 bytes, already packed
    python3 upload_photo.py picture.png -p /dev/ttyACM0

The badge stores 128x128 at one bit per pixel. Anything else is scaled to fit and
thresholded; --invert flips which side of the threshold becomes ink.
"""
import argparse, base64, glob, os, select, struct, sys, termios, time, zlib

W = H = 128
CHUNKS, CHUNK = 32, 64

def pack(bits):
    """bits[y][x] true where ink. -> 2048 bytes, big-endian u32 words."""
    words = [0] * (W * H // 32)
    for y in range(H):
        for x in range(W):
            if bits[y][x]:
                i = x + y * W
                words[i >> 5] |= 1 << (i & 31)
    return b"".join(struct.pack(">I", w) for w in words)

def from_image(path, invert):
    try:
        from PIL import Image
    except ImportError:
        sys.exit("Pillow needed for image files: pip install pillow (or use --raw)")
    im = Image.open(path).convert("L").resize((W, H))
    px = im.load()
    return [[(px[x, y] < 128) != invert for x in range(W)] for y in range(H)]

def open_port(path):
    fd = os.open(path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    i, o, c, l, isp, osp, cc = termios.tcgetattr(fd)
    i &= ~(termios.IXON | termios.IXOFF | termios.ICRNL | termios.INLCR | termios.IGNCR)
    o &= ~termios.OPOST
    l &= ~(termios.ICANON | termios.ECHO | termios.ECHOE | termios.ISIG)
    c |= termios.CLOCAL | termios.CREAD
    cc[termios.VMIN] = 0; cc[termios.VTIME] = 0
    termios.tcsetattr(fd, termios.TCSANOW, [i, o, c, l, isp, osp, cc])
    return fd

def talk(fd, line, wait=4.0):
    os.write(fd, line.encode() + b"\r\n")
    got, end = b"", time.time() + wait
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.2)
        if r:
            try: b = os.read(fd, 4096)
            except (BlockingIOError, OSError): continue
            got += b
            if b"OK" in got or b"SUCCESS" in got or b"ERR" in got:
                break
    return got.decode("utf-8", "replace")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("-p", "--port", help="default: first /dev/ttyACM*")
    ap.add_argument("--raw", action="store_true", help="input is 2048 packed bytes")
    ap.add_argument("--invert", action="store_true", help="swap ink and paper")
    a = ap.parse_args()

    if a.raw:
        data = open(a.image, "rb").read()
        if len(data) != W * H // 8:
            sys.exit(f"raw input must be {W * H // 8} bytes, got {len(data)}")
    else:
        data = pack(from_image(a.image, a.invert))

    port = a.port or next(iter(sorted(glob.glob("/dev/ttyACM*"))), None)
    if not port:
        sys.exit("no /dev/ttyACM* found - is the badge plugged in and booted?")
    fd = open_port(port)
    print(f"sending {a.image} to {port}")

    talk(fd, "photo put clear")
    for n in range(CHUNKS):
        body = struct.pack(">H", n) + data[n * CHUNK:(n + 1) * CHUNK]
        wire = body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)
        reply = talk(fd, "photo put " + base64.b64encode(wire).decode())
        if "ERR" in reply:
            sys.exit(f"chunk {n} refused: {reply.strip()}")
        print(f"\r  {n + 1}/{CHUNKS}", end="", flush=True)
    print()
    print("done - the photo is now in the badge's photo list")

main()
"##;

pub struct Photo {
    /// Chunks received so far for a `photo put`, indexed by chunk number.
    chunks: Vec<Option<[u8; CHUNK_DATA_SIZE]>>,
    received: usize,
    pddb: Pddb,
}
impl Photo {
    pub fn new() -> Self {
        Photo { chunks: vec![None; NUM_CHUNKS], received: 0, pddb: Pddb::new() }
    }

    fn clear(&mut self) {
        for slot in self.chunks.iter_mut() {
            *slot = None;
        }
        self.received = 0;
    }

    fn to_bitmap(&self) -> [u32; BITMAP_WORDS] {
        let mut bitmap = [0u32; BITMAP_WORDS];
        for (i, slot) in self.chunks.iter().enumerate() {
            let data = slot.as_ref().expect("chunk missing");
            let base = i * (CHUNK_DATA_SIZE / 4);
            for w in 0..(CHUNK_DATA_SIZE / 4) {
                let o = w * 4;
                bitmap[base + w] =
                    u32::from_be_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            }
        }
        bitmap
    }

    /// Store an assembled photo under the next free key.
    ///
    /// Mirrors the badge's own naming - photo_NNNN, monotonic so a delete cannot cause a
    /// collision - because the photos list is ordered by key and this has to slot into it.
    fn store(&mut self, words: &[u32; BITMAP_WORDS]) -> Option<String> {
        let existing = self.pddb.list_keys(VAULT_PHOTOS_DICT, None).unwrap_or_default();
        if existing.len() >= PHOTO_CAP {
            return None;
        }
        let next = existing
            .iter()
            .filter_map(|k| k.strip_prefix("photo_").and_then(|n| n.parse::<u32>().ok()))
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);
        let key = format!("photo_{:04}", next);
        let bytes: &[u8] = bytemuck::cast_slice(words);
        let mut k = self
            .pddb
            .get(VAULT_PHOTOS_DICT, &key, None, true, true, Some(PHOTO_BYTES), None::<fn()>)
            .ok()?;
        k.write_all(bytes).ok()?;
        self.pddb.sync().ok();
        Some(key)
    }
}

impl<'a> ShellCmdApi<'a> for Photo {
    cmd_api!(photo);

    fn help(&self) -> &'static str { "list, fetch and send photos" }

    fn usage(&self) -> &'static str {
        "photo list             how many are stored\n    \
         photo get <n>          photo n as a base64 BMP\n    \
         photo ascii <n>        photo n as ASCII art\n    \
         photo put <chunk>      send a photo to the badge, one base64 chunk at a time\n    \
         photo put clear        discard a half-sent photo\n    \
         photo script           print an uploader; photo script > upload_photo.py"
    }


    fn process(&mut self, args: String, env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        let mut ret = String::new();
        let helpstring =
            "photo [list | get <n> | ascii <n> | put <chunk> | put clear | script]";

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
            Some("script") => {
                // Printed rather than returned: the REPL's reply path is one line, and this
                // is a file. Redirect it - photo script > upload_photo.py
                println!("{}", UPLOAD_SCRIPT);
                write!(ret, "-- end of script --").unwrap();
            }
            Some("put") => {
                let arg = tokens.next().unwrap_or("");
                if arg == "clear" {
                    self.clear();
                    write!(ret, "CLEAR").unwrap();
                    return Ok(Some(ret));
                }
                let decoded = match B64.decode(arg) {
                    Ok(d) if d.len() == CHUNK_WIRE_SIZE => d,
                    _ => {
                        write!(ret, "ERR bad chunk").unwrap();
                        return Ok(Some(ret));
                    }
                };
                let index = u16::from_be_bytes([decoded[0], decoded[1]]) as usize;
                let want = u32::from_be_bytes([
                    decoded[66], decoded[67], decoded[68], decoded[69],
                ]);
                if crc32fast::hash(&decoded[..CHUNK_INDEX_BYTES + CHUNK_DATA_SIZE]) != want {
                    write!(ret, "ERR crc").unwrap();
                    return Ok(Some(ret));
                }
                if index >= NUM_CHUNKS {
                    write!(ret, "ERR index {}", index).unwrap();
                    return Ok(Some(ret));
                }
                let mut data = [0u8; CHUNK_DATA_SIZE];
                data.copy_from_slice(&decoded[CHUNK_INDEX_BYTES..CHUNK_INDEX_BYTES + CHUNK_DATA_SIZE]);
                if self.chunks[index].is_none() {
                    self.received += 1;
                }
                self.chunks[index] = Some(data);

                if self.received == NUM_CHUNKS {
                    let words = self.to_bitmap();
                    let stored = self.store(&words);
                    self.clear();
                    match stored {
                        Some(key) => write!(ret, "SUCCESS stored as {}", key).unwrap(),
                        None => write!(ret, "ERR photo store full").unwrap(),
                    }
                } else {
                    write!(ret, "OK {}/{}", self.received, NUM_CHUNKS).unwrap();
                }
            }
            _ => write!(ret, "{}", helpstring).unwrap(),
        }
        Ok(Some(ret))
    }
}
