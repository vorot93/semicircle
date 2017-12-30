extern crate enum_primitive;
extern crate radius_parser as rp;
extern crate std;

use errors;

use self::std::net::Ipv4Addr;
use self::rp::RadiusAttribute as rpAttr;
use self::rp::RadiusData as rpData;
use self::std::convert::TryFrom;
use self::enum_primitive::FromPrimitive;

#[derive(Clone, Debug, PartialEq)]
pub enum RadiusAttribute {
    UserName(String),
    UserPassword(String),
    ChapPassword(u8, String),
    NasIPAddress(Ipv4Addr),
    NasPort(u32),
    ServiceType(rp::ServiceType),
    FramedProtocol(rp::FramedProtocol),
    FramedIPAddress(Ipv4Addr),
    FramedIPNetmask(Ipv4Addr),
    FramedRouting(rp::FramedRouting),
    FilterId(String),
    FramedMTU(u32),
    FramedCompression(rp::FramedCompression),
    VendorSpecific(u32, String),
    CalledStationId(String),
    CallingStationId(String),

    Unknown(u8, Vec<u8>),
}

impl<'data> TryFrom<rpAttr<'data>> for RadiusAttribute {
    type Error = errors::Error;
    fn try_from(v: rpAttr) -> Result<Self, Self::Error> {
        Ok(match v {
            rpAttr::UserName(x) => RadiusAttribute::UserName(String::from_utf8(x.into())?),
            rpAttr::UserPassword(x) => RadiusAttribute::UserPassword(String::from_utf8(x.into())?),
            rpAttr::ChapPassword(x, y) => {
                RadiusAttribute::ChapPassword(x, String::from_utf8(y.into())?)
            }
            rpAttr::NasIPAddress(x) => RadiusAttribute::NasIPAddress(x),
            rpAttr::NasPort(x) => RadiusAttribute::NasPort(x),
            rpAttr::ServiceType(x) => RadiusAttribute::ServiceType(x),
            rpAttr::FramedProtocol(x) => RadiusAttribute::FramedProtocol(x),
            rpAttr::FramedIPAddress(x) => RadiusAttribute::FramedIPAddress(x),
            rpAttr::FramedIPNetmask(x) => RadiusAttribute::FramedIPNetmask(x),
            rpAttr::FramedRouting(x) => RadiusAttribute::FramedRouting(x),
            rpAttr::FilterId(x) => RadiusAttribute::FilterId(String::from_utf8(x.into())?),
            rpAttr::FramedMTU(x) => RadiusAttribute::FramedMTU(x),
            rpAttr::FramedCompression(x) => RadiusAttribute::FramedCompression(x),
            rpAttr::VendorSpecific(id, payload) => {
                RadiusAttribute::VendorSpecific(id, String::from_utf8(payload.into())?)
            }
            rpAttr::CalledStationId(x) => {
                RadiusAttribute::CalledStationId(String::from_utf8(x.into())?)
            }
            rpAttr::CallingStationId(x) => {
                RadiusAttribute::CallingStationId(String::from_utf8(x.into())?)
            }

            rpAttr::Unknown(id, payload) => RadiusAttribute::Unknown(id, payload.into()),
        })
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
