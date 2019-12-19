use radius_parser::RadiusAttribute as rpAttr;
use radius_parser::RadiusData as rpData;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::net::Ipv4Addr;

#[derive(Clone, Debug, PartialEq)]
pub struct VendorSpecificDecoded {
    pub vendor_type: u8,
    pub text: String,
}

impl<'data> TryFrom<&'data [u8]> for VendorSpecificDecoded {
    type Error = Box<dyn std::error::Error + Send + Sync>;
    fn try_from(v: &'data [u8]) -> Result<VendorSpecificDecoded, Self::Error> {
        let real_len = v.len();
        if real_len < 3 || real_len > 255 {
            Err("VSA data has invalid size".into())
        } else {
            let vendor_type = v[0];
            let len = v[1];

            if real_len - 3 != len as usize {
                Err("Invalid length in byte 2".into())
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
    pub fn try_decode(&self) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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
        let encoded = VendorSpecificData::Encoded(v);
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
    ServiceType(radius_parser::ServiceType),
    FramedProtocol(radius_parser::FramedProtocol),
    FramedIPAddress(Ipv4Addr),
    FramedIPNetmask(Ipv4Addr),
    FramedRouting(radius_parser::FramedRouting),
    FilterId(Vec<u8>),
    FramedMTU(u32),
    FramedCompression(radius_parser::FramedCompression),
    VendorSpecific(u32, VendorSpecificData),
    CalledStationId(Vec<u8>),
    CallingStationId(Vec<u8>),

    Unknown(u8, Vec<u8>),
}

impl<'data> TryFrom<rpAttr<'data>> for RadiusAttribute {
    type Error = Box<dyn std::error::Error + Send + Sync>;
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
    type Error = Box<dyn std::error::Error + Send + Sync>;
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
            RadiusAttribute::NasIPAddress(addr) => (4, crate::util::vec_from_ipv4(addr)),
            RadiusAttribute::NasPort(port) => (5, crate::util::vec_from_u32(port)),
            RadiusAttribute::ServiceType(id) => (6, crate::util::vec_from_u32(id.0)),
            RadiusAttribute::FramedProtocol(id) => (7, crate::util::vec_from_u32(id.0)),
            RadiusAttribute::FramedIPAddress(ip) => (8, crate::util::vec_from_ipv4(ip)),
            RadiusAttribute::FramedIPNetmask(ip) => (9, crate::util::vec_from_ipv4(ip)),
            RadiusAttribute::FramedRouting(id) => (10, crate::util::vec_from_u32(id.0)),
            RadiusAttribute::FilterId(text) => (11, text),
            RadiusAttribute::FramedMTU(mtu) => {
                if mtu < 64 || mtu > 65535 {
                    return Err("MTU out of range".to_string().into());
                } else {
                    (12, crate::util::vec_from_u32(mtu))
                }
            }
            RadiusAttribute::FramedCompression(id) => (12, crate::util::vec_from_u32(id.0)),
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
    type Error = Box<dyn std::error::Error + Send + Sync>;
    fn try_from(v: RadiusAttribute) -> Result<Vec<u8>, Self::Error> {
        let mut out = Vec::new();

        let (code, mut payload) = v.try_into()?;

        let len = payload.len() + 2;

        if len > 255 {
            return Err("Attribute length cannot exceed 255".to_string().into());
        }

        out.push(code);
        out.push(len as u8);
        out.append(&mut payload);

        Ok(out)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadiusData {
    pub code: radius_parser::RadiusCode,
    pub identifier: u8,
    pub authenticator: [u8; 16],
    pub attributes: Vec<RadiusAttribute>,
}

impl<'data> TryFrom<rpData<'data>> for RadiusData {
    type Error = Box<dyn std::error::Error + Send + Sync>;
    fn try_from(v: rpData) -> Result<Self, Self::Error> {
        let code = v.code;

        Ok(Self {
            code,
            identifier: v.identifier,
            authenticator: {
                let mut a: [u8; 16] = Default::default();
                a.copy_from_slice(&v.authenticator[0..16]);
                a
            },
            attributes: {
                let mut a = Vec::new();
                for attr in v.attributes.unwrap_or_else(Vec::new) {
                    a.push(RadiusAttribute::try_from(attr)?);
                }
                a
            },
        })
    }
}

impl From<RadiusData> for Vec<u8> {
    fn from(_v: RadiusData) -> Vec<u8> {
        vec![]
    }
}
