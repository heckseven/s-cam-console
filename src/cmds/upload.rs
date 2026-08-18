// upload.rs — hands the user the tool for putting things onto the badge.
//
// The badge carries its own uploader so nothing has to be found or downloaded first:
//
//     upload > upload.py
//     python3 upload.py
//
// The script asks what to send if it is not told, so this stays one command however many
// kinds of thing become uploadable.

use core::fmt::Write;

use String;

use crate::{CommonEnv, ShellCmdApi};

/// A self-contained uploader, kept on the badge so nobody has to go and find one.
///
/// `photo script > upload_photo.py` and it is ready to run. The wire format it speaks is the
/// one implemented below, and the bit packing matches what the camera writes - verified by
/// exporting a photo, repacking it with this code, and comparing.
const UPLOAD_SCRIPT: &str = r##"#!/usr/bin/env python3
"""Send things to an S-CAM badge over its serial console.

    python3 upload.py                       # asks what you want to send
    python3 upload.py picture.png           # a photo (needs Pillow)
    python3 upload.py --qr https://a.com    # a QR code
    python3 upload.py picture.raw --raw     # 2048 bytes, already packed

Photos are stored 128x128 at one bit per pixel; anything else is scaled to fit and
thresholded, and --invert flips which side of the threshold becomes ink.
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
    ap.add_argument("image", nargs="?", help="image file; omit to be asked")
    ap.add_argument("-p", "--port", help="default: first /dev/ttyACM*")
    ap.add_argument("--raw", action="store_true", help="input is 2048 packed bytes")
    ap.add_argument("--invert", action="store_true", help="swap ink and paper")
    ap.add_argument("--qr", metavar="URL", help="save a QR code instead of a photo")
    a = ap.parse_args()

    port = a.port or next(iter(sorted(glob.glob("/dev/ttyACM*"))), None)
    if not port:
        sys.exit("no /dev/ttyACM* found - is the badge plugged in and booted?")

    # Nothing named: ask, rather than printing usage and quitting. The point of this script
    # living on the badge is that it can be run without reading anything first.
    if not a.image and not a.qr:
        print("What do you want to send to the badge?")
        print("  1. a photo")
        print("  2. a QR code")
        choice = input("> ").strip()
        if choice.startswith("2"):
            a.qr = input("URL: ").strip()
        else:
            a.image = input("image file: ").strip()

    if a.qr:
        fd = open_port(port)
        reply = talk(fd, "qr add " + a.qr, 6.0)
        print(reply.strip().splitlines()[-1] if reply.strip() else "no reply")
        return

    if a.raw:
        data = open(a.image, "rb").read()
        if len(data) != W * H // 8:
            sys.exit(f"raw input must be {W * H // 8} bytes, got {len(data)}")
    else:
        data = pack(from_image(a.image, a.invert))

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

pub struct Upload {}
impl Upload {
    pub fn new() -> Self { Upload {} }
}

impl<'a> ShellCmdApi<'a> for Upload {
    cmd_api!(upload);

    fn help(&self) -> &'static str { "print a script for sending things to the badge" }

    fn usage(&self) -> &'static str {
        "upload > upload.py     save it, then run: python3 upload.py"
    }

    fn process(&mut self, _args: String, _env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        // Printed rather than returned: the reply path is one line and this is a file.
        //
        // A line at a time, with a pause every so often. Printing the whole script in one
        // call overran the serial buffer and the *beginning* was lost - the tail arrived
        // looking like a complete file that started halfway through a function, which is a
        // nasty thing to hand somebody as a script to run.
        for (n, line) in UPLOAD_SCRIPT.lines().enumerate() {
            println!("{}", line);
            if n % 8 == 7 {
                _env.ticktimer.sleep_ms(20).ok();
            }
        }
        let mut ret = String::new();
        write!(ret, "-- end of script --").unwrap();
        Ok(Some(ret))
    }
}
