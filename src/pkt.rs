extern crate enum_primitive;
extern crate radius_parser as rp;
extern crate std;

use errors;
use util;

use self::std::net::Ipv4Addr;
use self::rp::RadiusAttribute as rpAttr;
use self::rp::RadiusData as rpData;
use self::std::convert::TryFrom;
use self::std::convert::TryInto;
use self::enum_primitive::FromPrimitive;

#[derive(Clone, Debug, PartialEq)]
pub struct VendorSpecificDecoded {
    pub vendor_type: u8,
    pub text: String,
}

impl<'data> TryFrom<&'data [u8]> for VendorSpecificDecoded {
    type Error = errors::Error;
    fn try_from(v: &'data [u8]) -> Result<VendorSpecificDecoded, Self::Error> {
        let real_len = v.len();
        if real_len < 3 || real_len > 255 {
            Err(errors::ErrorKind::ParseError("VSA data has invalid size".into()).into())
        } else {
            let vendor_type = v[0];
            let len = v[1];

            if real_len - 3 != len as usize {
                Err(errors::ErrorKind::ParseError("Invalid length in byte 2".into()).into())
            } else {
                let text = String::from_utf8(v.get(2..real_len - 1).unwrap().into())?;
                Ok(VendorSpecificDecoded { vendor_type, text })
            }
        }
    }
}

impl From<VendorSpecificDecoded> for Vec<u8> {
    fn from(v: VendorSpecificDecoded) -> Vec<u8> {
        let mut text = v.text.into_bytes();
        text.truncate(247);

        let mut out = Vec::new();
        out.push(v.vendor_type);
        out.push(text.len() as u8);
        out.append(&mut text);

        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VendorSpecificData {
    Decoded(VendorSpecificDecoded),
    Encoded(Vec<u8>),
}

impl VendorSpecificData {
    pub fn try_decode(&self) -> errors::Result<Self> {
        match *self {
            VendorSpecificData::Decoded(_) => Ok(self.clone()),
            VendorSpecificData::Encoded(ref data) => Ok(VendorSpecificData::Decoded(
                VendorSpecificDecoded::try_from(data.as_slice())?,
            )),
        }
    }

    pub fn encode(self) -> Self {
        match self {
            VendorSpecificData::Decoded(data) => VendorSpecificData::Encoded(data.into()),
            VendorSpecificData::Encoded(_) => self,
        }
    }
}

impl From<Vec<u8>> for VendorSpecificData {
    fn from(v: Vec<u8>) -> VendorSpecificData {
        let encoded = VendorSpecificData::Encoded(v.into());
        match encoded.try_decode() {
            Ok(decoded) => decoded,
            Err(_) => encoded,
        }
    }
}

impl From<VendorSpecificData> for Vec<u8> {
    fn from(v: VendorSpecificData) -> Vec<u8> {
        match v {
            VendorSpecificData::Decoded(data) => data.into(),
            VendorSpecificData::Encoded(data) => data,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RadiusAttribute {
    UserName(Vec<u8>),
    UserPassword(Vec<u8>),
    ChapPassword(u8, [u8; 16]),
    NasIPAddress(Ipv4Addr),
    NasPort(u32),
    ServiceType(rp::ServiceType),
    FramedProtocol(rp::FramedProtocol),
    FramedIPAddress(Ipv4Addr),
    FramedIPNetmask(Ipv4Addr),
    FramedRouting(rp::FramedRouting),
    FilterId(Vec<u8>),
    FramedMTU(u32),
    FramedCompression(rp::FramedCompression),
    VendorSpecific(u32, VendorSpecificData),
    CalledStationId(Vec<u8>),
    CallingStationId(Vec<u8>),

    Unknown(u8, Vec<u8>),
}

impl<'data> TryFrom<rpAttr<'data>> for RadiusAttribute {
    type Error = errors::Error;
    fn try_from(v: rpAttr) -> Result<Self, Self::Error> {
        Ok(match v {
            rpAttr::UserName(name) => RadiusAttribute::UserName(name.into()),
            rpAttr::UserPassword(password) => RadiusAttribute::UserPassword(password.into()),
            rpAttr::ChapPassword(ident, unowned_password) => {
                let mut password: [u8; 16] = Default::default();
                password.copy_from_slice(&unowned_password[0..16]);
                RadiusAttribute::ChapPassword(ident, password)
            }
            rpAttr::NasIPAddress(addr) => RadiusAttribute::NasIPAddress(addr),
            rpAttr::NasPort(port) => RadiusAttribute::NasPort(port),
            rpAttr::ServiceType(service) => RadiusAttribute::ServiceType(service),
            rpAttr::FramedProtocol(protocol) => RadiusAttribute::FramedProtocol(protocol),
            rpAttr::FramedIPAddress(addr) => RadiusAttribute::FramedIPAddress(addr),
            rpAttr::FramedIPNetmask(mask) => RadiusAttribute::FramedIPNetmask(mask),
            rpAttr::FramedRouting(routing) => RadiusAttribute::FramedRouting(routing),
            rpAttr::FilterId(id) => RadiusAttribute::FilterId(id.into()),
            rpAttr::FramedMTU(mtu) => RadiusAttribute::FramedMTU(mtu),
            rpAttr::FramedCompression(compression) => {
                RadiusAttribute::FramedCompression(compression)
            }
            rpAttr::VendorSpecific(id, payload) => {
                RadiusAttribute::VendorSpecific(id, Vec::from(payload).into())
            }
            rpAttr::CalledStationId(id) => RadiusAttribute::CalledStationId(id.into()),
            rpAttr::CallingStationId(id) => RadiusAttribute::CallingStationId(id.into()),

            rpAttr::Unknown(id, payload) => RadiusAttribute::Unknown(id, payload.into()),
        })
    }
}

impl TryFrom<RadiusAttribute> for (u8, Vec<u8>) {
    type Error = errors::Error;
    fn try_from(v: RadiusAttribute) -> Result<Self, Self::Error> {
        Ok(match v {
            RadiusAttribute::UserName(val) => (1, val),
            RadiusAttribute::UserPassword(val) => (2, val),
            RadiusAttribute::ChapPassword(ident, password) => {
                let mut out = Vec::new();
                out.push(ident);
                out.extend_from_slice(&password);
                (3, out)
            }
            RadiusAttribute::NasIPAddress(addr) => (4, util::vec_from_ipv4(addr)),
            RadiusAttribute::NasPort(port) => (5, util::vec_from_u32(port)),
            RadiusAttribute::ServiceType(id) => (6, util::vec_from_u32(id as u32)),
            RadiusAttribute::FramedProtocol(id) => (7, util::vec_from_u32(id as u32)),
            RadiusAttribute::FramedIPAddress(ip) => (8, util::vec_from_ipv4(ip)),
            RadiusAttribute::FramedIPNetmask(ip) => (9, util::vec_from_ipv4(ip)),
            RadiusAttribute::FramedRouting(id) => (10, util::vec_from_u32(id as u32)),
            RadiusAttribute::FilterId(text) => (11, text),
            RadiusAttribute::FramedMTU(mtu) => {
                if mtu < 64 || mtu > 65535 {
                    return Err(errors::ErrorKind::ParseError("MTU out of range".into()).into());
                } else {
                    (12, util::vec_from_u32(mtu))
                }
            }
            RadiusAttribute::FramedCompression(id) => (12, util::vec_from_u32(id as u32)),
            RadiusAttribute::VendorSpecific(vendor_id, data) => {
                let mut out = vec![];
                out.push(vendor_id as u8);
                out.append(&mut data.into());
                (26, out)
            }
            RadiusAttribute::CalledStationId(data) => (30, data),
            RadiusAttribute::CallingStationId(data) => (31, data),
            RadiusAttribute::Unknown(id, data) => (id, data),
        })
    }
}

impl TryFrom<RadiusAttribute> for Vec<u8> {
    type Error = errors::Error;
    fn try_from(v: RadiusAttribute) -> Result<Vec<u8>, Self::Error> {
        let mut out = Vec::new();

        let (code, mut payload) = v.try_into()?;

        let len = payload.len() + 2;

        if len > 255 {
            return Err(
                errors::ErrorKind::ParseError("Attribute length cannot exceed 255".into()).into(),
            );
        }

        out.push(code);
        out.push(len as u8);
        out.append(&mut payload);

        Ok(out)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadiusData {
    pub code: self::rp::RadiusCode,
    pub identifier: u8,
    pub authenticator: [u8; 16],
    pub attributes: Vec<RadiusAttribute>,
}

impl<'data> TryFrom<rpData<'data>> for RadiusData {
    type Error = errors::Error;
    fn try_from(v: rpData) -> Result<Self, Self::Error> {
        Ok(Self {
            code: self::rp::RadiusCode::from_u8(v.code)?,
            identifier: v.identifier,
            authenticator: {
                let mut a: [u8; 16] = Default::default();
                a.copy_from_slice(&v.authenticator[0..16]);
                a
            },
            attributes: {
                let mut a = Vec::new();
                for attr in v.attributes.unwrap_or(vec![]) {
                    a.push(RadiusAttribute::try_from(attr)?);
                }
                a
            },
        })
    }
}

impl From<RadiusData> for Vec<u8> {
    fn from(v: RadiusData) -> Vec<u8> {
        let out = Vec::new();

        out
    }
}
