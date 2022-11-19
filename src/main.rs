use anyhow::{bail, Result};
use inflate::inflate_bytes;

struct Reader<'d> {
    data: &'d [u8],
    offset: usize,
}

impl<'d> Reader<'d> {
    fn new(data: &'d [u8]) -> Self {
        Self {
            data,
            offset: 0,
        }
    }

    fn read_n<const N: usize>(&mut self) -> Result<&'d [u8; N]> {
        Ok(self.read_slice(N)?.try_into()?)
    }

    fn peek_n<const N: usize>(&mut self) -> Result<&'d [u8; N]> {
        Ok(self.peek_slice(N)?.try_into()?)
    }

    fn read_u32(&mut self) -> Result<u32> {
        self.read_n().copied().map(u32::from_le_bytes)
    }

    fn read_u16(&mut self) -> Result<u16> {
        self.read_n().copied().map(u16::from_le_bytes)
    }

    fn read_slice(&mut self, n: usize) -> Result<&'d [u8]> {
        let data = self.peek_slice(n)?;
        self.offset += n;
        Ok(data)
    }

    fn peek_slice(&self, n: usize) -> Result<&'d [u8]> {
        if self.offset + n <= self.data.len() {
            Ok(&self.data[self.offset..][..n])
        } else {
            bail!("End of stream")
        }
    }

    fn ignore_while<const N: usize>(&mut self, f: impl Fn(&[u8; N]) -> bool) {
        while self.offset + N < self.data.len() {
            let window: &[u8; N] = self.data[self.offset..][..N].try_into().unwrap();
            if !f(window) {
                break;
            }
            self.offset += 1;
        }
    }
}

fn main() -> anyhow::Result<()> {
    let data = std::fs::read(r"C:\Users\Host\Downloads\zplayer")?;
    let mut reader = Reader::new(&data);
    let Ok(b"PK\x03\x04") = reader.read_n() else {
        anyhow::bail!("uwu");
    };
    let Ok([0x14, 00]) = reader.read_n() else {
        anyhow::bail!("ara");
    };

    let flags = reader.read_u16()?;
    let compression = reader.read_u16()?;
    let modtime = reader.read_u16()?;
    let moddate = reader.read_u16()?;
    let crc32 = reader.read_u32()?;
    let compressed = reader.read_u32()?;
    let uncompressed = reader.read_u32()?;
    let name_len = reader.read_u16()?;
    let extra_len = reader.read_u16()?;
    let name = reader.read_slice(usize::from(name_len))?;
    let extra_len = reader.read_slice(usize::from(extra_len))?;
    let data = reader.read_slice(compressed as _)?;
    // println!(
    //     flags,
    //     compression,
    //     modtime,
    //     moddate,
    //     crc32,
    //     compressed,
    //     uncompressed,
    //     name_len,name
    // );

    let data = inflate_bytes(data).map_err(anyhow::Error::msg)?;
    assert!(data.len() == uncompressed as _);

    let mut reader = Reader::new(&data);
    let marker = reader.read_n::<0x14>()?;
    // println!("header: {:02X?}", marker);

    let mut stack = vec![4];
    let mut next_is_key = true;

    while let Ok([code]) = reader.read_n() {
        if next_is_key {
            print!("{}", " ".repeat(stack.len()));
        } else {
            print!(": ");
        }

        match code {
            0x05 => {
                let data = u32::from_le_bytes(*reader.read_n()?);
                print!("{}", data);
            }
            0x06 => {
                let data = u64::from_le_bytes(*reader.read_n()?);
                print!("{}", data);
            }
            0x07 => {
                // dict
                let dat = u32::from_be_bytes(*reader.read_n::<4>()?);
                stack.push(dat);
                print!("{{");
            }
            0x08 => {
                let len = reader.read_n::<2>()?[1];
                let val = reader.read_slice(usize::from(len))?;
                let val = std::str::from_utf8(val)?;
                print!("{}", val);
            }
            0x0A => {
                let data = *reader.read_n()? == [1];
                print!("{}", data);
            }
            0x0B => {
                let data = u16::from_le_bytes(*reader.read_n()?);
                print!("{}", data);
            }
            0x0C => {
                let dat = reader.read_n::<8>()?;
                print!("{:02X?}", dat);
            }
            0x0D => {
                let dat = reader.read_n::<0xC>()?;
                print!("{:02X?}", dat);
            }
            _ => {
                print!("???{:02X} {:02X?}", code, reader.peek_slice(0x30));
                break;
            }
        }

        if !next_is_key {
            println!();

            while let Some(0) = stack.last() {
                stack.pop();
                print!("{}", " ".repeat(stack.len()));
                println!("}}");
            }

            if stack.len() > 0 {
                let mut top = stack.len() - 1;
                while stack[top] == 0 {
                    top -= 1;
                }
                stack[top] -= 1;
            }
        }
        next_is_key ^= true;
    }

    Ok(())
}