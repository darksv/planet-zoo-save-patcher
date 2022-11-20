use inflate::inflate_bytes;

use crate::crc32::game_crc;

mod crc32;

struct Reader<'d> {
    data: &'d [u8],
    offset: usize,
}

#[allow(unused)]
impl<'d> Reader<'d> {
    fn new(data: &'d [u8]) -> Self {
        Self {
            data,
            offset: 0,
        }
    }

    fn read_n<const N: usize>(&mut self) -> anyhow::Result<&'d [u8; N]> {
        Ok(self.read_slice(N)?.try_into()?)
    }

    fn remainder(&self) -> &'d [u8] {
        &self.data[self.offset..]
    }

    fn peek_n<const N: usize>(&mut self) -> anyhow::Result<&'d [u8; N]> {
        Ok(self.peek_slice(N)?.try_into()?)
    }

    fn read_u32(&mut self) -> anyhow::Result<u32> {
        self.read_n().copied().map(u32::from_le_bytes)
    }

    fn read_u32_be(&mut self) -> anyhow::Result<u32> {
        self.read_n().copied().map(u32::from_be_bytes)
    }

    fn read_u16(&mut self) -> anyhow::Result<u16> {
        self.read_n().copied().map(u16::from_le_bytes)
    }

    fn read_slice(&mut self, n: usize) -> anyhow::Result<&'d [u8]> {
        let data = self.peek_slice(n)?;
        self.offset += n;
        Ok(data)
    }

    fn peek_slice(&self, n: usize) -> anyhow::Result<&'d [u8]> {
        if self.offset + n <= self.data.len() {
            Ok(&self.data[self.offset..][..n])
        } else {
            anyhow::bail!("End of stream")
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

#[derive(Debug)]
enum Data<'d> {
    Table(Vec<(Data<'d>, Data<'d>)>),
    Uint64(u64),
    Uint32(u32),
    Uint16(u16),
    Boolean(bool),
    Unknown(u8, &'d [u8]),
    String(&'d str),
}

impl<'d> Data<'d> {
    fn as_str(&self) -> Option<&str> {
        match self {
            Data::String(x) => Some(x),
            _ => None,
        }
    }

    fn as_items_mut(&mut self) -> Option<&mut [(Data<'d>, Data<'d>)]> {
        match self {
            Data::Table(x) => Some(x),
            _ => None,
        }
    }

    fn get<'item>(&'item mut self, name: &str) -> Option<&'item mut Data<'d>>
        where 'd: 'item
    {
        match self {
            Data::Table(items) => {
                for (key, value) in items {
                    if key.as_str() == Some(name) {
                        return Some(value);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn set_field<'i>(&mut self, name: &'i str, value: Data<'i>) where 'i: 'd {
        match self {
            Data::Table(values) => {
                if let Some((k, v)) = values.iter_mut().find(|(k, v)| k.as_str() == Some(name)) {
                    *v = value;
                } else {
                    values.push((Data::String(name), value));
                }
            }
            _ => unimplemented!(),
        }
    }
}


#[derive(Debug)]
struct File<'d> {
    data: Data<'d>,
}

const MAGIC: [u8; 4] = [0xFF, 0x00, 0xFE, 0x01];

fn deserialize(data: &[u8]) -> anyhow::Result<File> {
    let mut reader = Reader::new(&data);
    let &MAGIC = reader.read_n::<4>()? else {
        anyhow::bail!("Invalid magic value");
    };

    let crc = reader.read_u32_be()?;
    let num = reader.read_u32_be()?;
    let len = reader.read_u32_be()?;

    if crc != game_crc(reader.peek_slice(len as usize)?) {
        anyhow::bail!("Invalid crc");
    }

    if num != 1 {
        anyhow::bail!("Invalid number of files");
    }

    let n = reader.read_u32_be()?;
    let mut items = Vec::new();
    for _ in 0..n {
        let d = read_data(&mut reader)?;
        let y = read_data(&mut reader)?;
        items.push((d, y));
    }

    Ok(File {
        data: Data::Table(items),
    })
}

fn serialize(file: &File, out: &mut Vec<u8>) {
    out.extend(MAGIC);
    out.extend([0; 12]);
    let before = out.len();
    serialize_data(&file.data, out, true);
    let data_len = out.len() - before;
    let crc = game_crc(&out[before..]) as u32;
    out[4..][..4].copy_from_slice(&u32::to_be_bytes(crc));
    out[8..][..4].copy_from_slice(&u32::to_be_bytes(1));
    out[12..][..4].copy_from_slice(&u32::to_be_bytes(data_len as u32));
}

fn modify(file: &mut File) {
    let progress = file.data
        .get("Data").unwrap()
        .get("Career").unwrap()
        .get("Progress").unwrap();

    for (k, v) in progress.as_items_mut().unwrap() {
        match k.as_str().unwrap() {
            "Challenge_06" |
            "Challenge_07" |
            "Challenge_08" |
            "Challenge_09" => {
                for (k, v) in v.as_items_mut().unwrap() {
                    println!("BEFORE {:?}", v);
                    v.set_field("bComplete", Data::Boolean(true));
                    println!("AFTER {:?}", v);
                }
            }
            _ => (),
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
    let extra = reader.read_slice(usize::from(extra_len))?;
    let data = reader.read_slice(compressed as _)?;
    dbg!(flags, compression, modtime, moddate, crc32, compressed, uncompressed, name_len, extra_len, std::str::from_utf8(name), extra);

    let data_in = inflate_bytes(data).map_err(anyhow::Error::msg)?;
    assert_eq!(data_in.len(), uncompressed as _);

    let mut file = deserialize(&data_in)?;
    modify(&mut file);
    let mut content = Vec::new();
    serialize(&file, &mut content);
    std::fs::write("content", &content)?;

    // assert_eq!(&content, &data_in);


    let packed = deflate::deflate_bytes(&content);
    dbg!(packed.len());

    let mut out = Vec::new();
    out.extend_from_slice(HEAD);
    out.extend_from_slice(&packed);
    out.extend_from_slice(TAIL);
    out[0x0E..][..4].copy_from_slice(&u32::to_le_bytes(crc32fast::hash(&content)));
    out[0x12..][..4].copy_from_slice(&u32::to_le_bytes(packed.len() as _));
    out[0x16..][..4].copy_from_slice(&u32::to_le_bytes(content.len() as _));

    std::fs::write("patched", out)?;
    Ok(())
}

fn serialize_data(data: &Data, out: &mut Vec<u8>, is_first: bool) {
    match data {
        Data::Table(items) => {
            if !is_first {
                out.push(0x07);
            }
            out.extend((items.len() as u32).to_be_bytes());
            for it in items {
                serialize_data(&it.0, out, false);
                serialize_data(&it.1, out, false);
            }
        }
        Data::Uint64(val) => {
            out.push(0x06);
            out.extend(val.to_le_bytes());
        }
        Data::Uint32(val) => {
            out.push(0x05);
            out.extend(val.to_le_bytes());
        }
        Data::Uint16(val) => {
            out.push(0x0B);
            out.extend(val.to_le_bytes());
        }
        Data::Boolean(val) => {
            out.push(0x0A);
            out.extend([*val as u8]);
        }
        Data::Unknown(code, pl) => {
            out.extend([*code]);
            out.extend_from_slice(pl);
        }
        Data::String(val) => {
            out.extend([0x08, 0x00]);
            out.extend([val.len() as u8]);
            out.extend(val.as_bytes());
        }
    }
}

const HEAD: &[u8] = &[
    0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x08, 0x00, 0x72, 0xA2, 0x71, 0x55, 0x4D, 0x0D,
    0x44, 0x3E, 0xEF, 0x13, 0x00, 0x00, 0xAE, 0x4C, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x67, 0x61,
    0x6D, 0x65, 0x64, 0x61, 0x74, 0x61
];

const TAIL: &[u8] = &[
    0x50, 0x4B, 0x01, 0x02, 0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x08, 0x00, 0x72, 0xA2, 0x71, 0x55,
    0x4D, 0x0D, 0x44, 0x3E, 0xEF, 0x13, 0x00, 0x00, 0xAE, 0x4C, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x67, 0x61,
    0x6D, 0x65, 0x64, 0x61, 0x74, 0x61, 0x50, 0x4B, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x15, 0x14, 0x00, 0x00, 0x00, 0x00
];

fn read_data<'d>(reader: &mut Reader<'d>) -> anyhow::Result<Data<'d>> {
    let &[code] = reader.read_n()?;
    let data = match code {
        0x05 => {
            let data = u32::from_le_bytes(*reader.read_n()?);
            Data::Uint32(data)
        }
        0x06 => {
            let data = u64::from_le_bytes(*reader.read_n()?);
            Data::Uint64(data)
        }
        0x07 => {
            // dict
            let dat = u32::from_be_bytes(*reader.read_n::<4>()?);
            let mut item = Vec::new();
            for _ in 0..dat {
                item.push((read_data(reader)?, read_data(reader)?));
            }
            Data::Table(item)
        }
        0x08 => {
            let &[x, len] = reader.read_n::<2>()?;
            assert_eq!(x, 0);
            let val = reader.read_slice(usize::from(len))?;
            Data::String(std::str::from_utf8(val).unwrap())
        }
        0x0A => {
            let data = *reader.read_n()? == [1];
            Data::Boolean(data)
        }
        0x0B => {
            let data = u16::from_le_bytes(*reader.read_n()?);
            Data::Uint16(data)
        }
        0x0C => {
            let dat = reader.read_n::<8>()?;
            Data::Unknown(code, dat)
        }
        0x0D => {
            let dat = reader.read_n::<0xC>()?;
            Data::Unknown(code, dat)
        }
        _ => {
            unimplemented!("???{:02X} {:02X?}", code, reader.peek_slice(0x30))
        }
    };
    Ok(data)
}