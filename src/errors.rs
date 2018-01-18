extern crate std;

error_chain! {
    foreign_links {
        IOError(std::io::Error);
        Utf8Error(std::string::FromUtf8Error);
    }
    errors {
        ParseError(t: String) {
            description(""),
            display("{}", t),
        }
        CancelledError(t: String) {
            description("operation has been cancelled"),
            display("operation has been cancelled: {}", t)
        }
    }
}

impl From<std::option::NoneError> for Error {
    fn from(v: std::option::NoneError) -> Error {
        ErrorKind::ParseError(format!("{:?}", v)).into()
    }
}
