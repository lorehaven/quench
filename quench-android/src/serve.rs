//! The program's side of the protocol: frames in on standard input, frames out on standard output.

use std::io::{self, Read, Write};
use std::process::ExitCode;

/// The largest frame taken. A request this size is a mistake or an attack, not a job.
pub const MAX_FRAME: usize = 64 << 20;

/// Reads one frame, or `None` at the clean end of the input.
fn read_frame<R: Read>(input: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0_u8; 4];
    let mut got = 0;
    while got < length.len() {
        match input.read(&mut length[got..])? {
            0 if got == 0 => return Ok(None),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "a frame ended inside its length",
                ));
            }
            n => got += n,
        }
    }
    let size = u32::from_be_bytes(length) as usize;
    if size > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a frame of {size} bytes is over the limit"),
        ));
    }
    let mut frame = vec![0_u8; size];
    input.read_exact(&mut frame)?;
    Ok(Some(frame))
}

fn write_frame<W: Write>(output: &mut W, bytes: &[u8]) -> io::Result<()> {
    let size = u32::try_from(bytes.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "the answer is too large"))?;
    output.write_all(&size.to_be_bytes())?;
    output.write_all(bytes)?;
    output.flush()
}

/// Answers requests until the input ends.
///
/// # Errors
/// A request cut short or too large, or an output that cannot be written.
pub fn serve<R: Read, W: Write>(input: &mut R, output: &mut W) -> io::Result<()> {
    while let Some(op) = read_frame(input)? {
        let data = read_frame(input)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::UnexpectedEof, "a request has no input frame")
        })?;
        let result = String::from_utf8(op).map_or_else(
            |_| Err("the operation name is not text".to_string()),
            |op| crate::call(&op, &data),
        );
        write_frame(output, &crate::encode(result))?;
    }
    Ok(())
}

/// The `main` of an app's native program: runs `init` (where the operations are registered), then
/// serves standard input and output until the host closes them.
#[must_use]
pub fn run(init: fn()) -> ExitCode {
    init();
    let (mut input, mut output) = (io::stdin().lock(), io::stdout().lock());
    let served = serve(&mut input, &mut output);
    match served {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("quench native: {e}");
            ExitCode::FAILURE
        }
    }
}
