pub const SYSTEM_READ_DIRS: &[&str] = &[
    "/usr", "/lib", "/lib64", "/bin", "/sbin",
    "/usr/local/lib", "/usr/local/share",
    "/usr/share/locale", "/usr/share/zoneinfo",
];

pub const PROC_READ_PATHS: &[&str] = &[
    "/proc/self",
    "/proc/cpuinfo", "/proc/meminfo",
    "/proc/filesystems",
    // Node.js reads cgroup memory limits
    "/sys/fs/cgroup",
];

pub const READ_WRITE_DIRS: &[&str] = &["/tmp"];

pub const READ_WRITE_DEVICES: &[&str] = &[
    "/dev/null", "/dev/zero", "/dev/urandom", "/dev/random",
];
