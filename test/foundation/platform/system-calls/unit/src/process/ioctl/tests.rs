#[test]
fn tiocsctty_argument_is_zero() {
    use super::Tiocsctty;
    use crate::ioctl::Ioctl;

    assert!(Tiocsctty.as_ptr().is_null());
}
