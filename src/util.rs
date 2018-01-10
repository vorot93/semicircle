extern crate byteorder;
extern crate std;

use self::byteorder::ByteOrder;
use self::std::net::Ipv4Addr;

pub fn vec_from_u32(v: u32) -> Vec<u8> {
    let mut buf = [0; 4];
    byteorder::BigEndian::write_u32(&mut buf, v);

    let mut out = Vec::new();
    out.extend_from_slice(&buf);

    out
}

pub fn vec_from_ipv4(v: Ipv4Addr) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&v.octets());
    out
}
