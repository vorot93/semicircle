extern crate failure;
extern crate std;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "operation has been cancelled: {}", reason)]
    CancelledError { reason: String },
}
