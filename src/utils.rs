use nix::{sys::ptrace, unistd::Pid};

const WORD_SIZE: usize = size_of::<usize>();

/// Writes the buffer `buf` to `addr` in the thread's memory
/// Returns `Ok(())` if all the bytes were written.
/// In an error happend during writing, Err(n) contains `n`, the number of bytes written.
pub fn write_data(pid: Pid, addr: usize, buf: &[u8]) -> Result<(), usize> {
    for bytes_written in (0..buf.len()).step_by(WORD_SIZE) {
        let rest = buf.len() - bytes_written;
        if rest > WORD_SIZE {
            // we have more that WORD_SIZE bytes to write, wa can simply write the entire next word
            let mut data: [u8; WORD_SIZE] = [0; WORD_SIZE];
            data.copy_from_slice(&buf[bytes_written..bytes_written + 4]);
            let data = usize::from_ne_bytes(data);
            ptrace::write(pid, (addr + bytes_written) as _, data as _)
                .map_err(|_| bytes_written)?;
        } else {
            // we have less than WORD_SIZE bytes to write, we must copy the existing data in order to not overwriting it
            let present_data =
                ptrace::read(pid, (addr + bytes_written) as _).map_err(|_| bytes_written)?;
            let mut present_data = present_data.to_ne_bytes();
            present_data[0..rest].copy_from_slice(&buf[bytes_written..]);
            let data = usize::from_ne_bytes(present_data);
            ptrace::write(pid, (addr + bytes_written) as _, data as _)
                .map_err(|_| bytes_written)?;
        }
    }
    Ok(())
}

// Reads `N` bytes if thread's memory into buffer
pub fn read_data_fixed<const N: usize>(pid: Pid, addr: usize) -> Option<[u8; N]> {
    let mut res: [u8; N] = [0; N];
    for bytes_read in (0..N).step_by(WORD_SIZE) {
        let data = ptrace::read(pid, (addr + bytes_read) as _).ok()?;
        let rest = N - bytes_read;
        if rest > WORD_SIZE {
            res[bytes_read..bytes_read + WORD_SIZE].copy_from_slice(&data.to_ne_bytes());
        } else {
            res[bytes_read..].copy_from_slice(&data.to_ne_bytes()[..rest]);
        }
    }
    Some(res)
}

// Reads `n` bytes if thread's memory into buffer
pub fn read_data(pid: Pid, addr: usize, n: usize) -> Option<Vec<u8>> {
    let mut res = Vec::with_capacity(n);
    for bytes_read in (0..n).step_by(WORD_SIZE) {
        let data = ptrace::read(pid, (addr + bytes_read) as _).ok()?;
        let rest = n - bytes_read;
        if rest > WORD_SIZE {
            res.extend_from_slice(&data.to_ne_bytes());
        } else {
            res.extend_from_slice(&data.to_ne_bytes()[..rest])
        }
    }
    Some(res)
}
