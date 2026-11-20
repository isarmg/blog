
    use super::*;
    use crate::fd::AsRawFd as _;

    #[test]
    fn test_cwd() {
        assert!(CWD.as_raw_fd() != -1);
        assert!(CWD.as_raw_fd() != c::STDIN_FILENO);
        assert!(CWD.as_raw_fd() != c::STDOUT_FILENO);
        assert!(CWD.as_raw_fd() != c::STDERR_FILENO);
    }

    #[test]
    fn test_abs() {
        assert!(ABS.as_raw_fd() < 0);
        assert!(ABS.as_raw_fd() != -1);
        assert!(ABS.as_raw_fd() != c::AT_FDCWD);
        assert!(ABS.as_raw_fd() != c::STDIN_FILENO);
        assert!(ABS.as_raw_fd() != c::STDOUT_FILENO);
        assert!(ABS.as_raw_fd() != c::STDERR_FILENO);
    }
