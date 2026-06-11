use std::net::Ipv4Addr;

pub fn vec_from_u32(v: u32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

pub fn vec_from_ipv4(v: Ipv4Addr) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&v.octets());
    out
}
