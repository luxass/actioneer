const std = @import("std");

pub const Environment = struct {
    ci: bool = false,
    verbose: bool = false,

    pub fn from(overrides: Overrides) Environment {
        return .{
            .ci = boolEnv("CI"),
            .verbose = overrides.verbose or boolEnv("VERBOSE"),
        };
    }
};

pub const Overrides = struct {
    verbose: bool = false,
};

var current: Environment = .{};

pub fn init(overrides: Overrides) void {
    current = Environment.from(overrides);
}

pub fn isCi() bool {
    return current.ci;
}

pub fn isVerbose() bool {
    return current.verbose;
}

pub fn boolEnv(name: [*:0]const u8) bool {
    const value = std.c.getenv(name) orelse return false;
    const slice = std.mem.sliceTo(value, 0);
    return std.ascii.eqlIgnoreCase(slice, "true") or
        std.mem.eql(u8, slice, "1") or
        std.ascii.eqlIgnoreCase(slice, "yes") or
        std.ascii.eqlIgnoreCase(slice, "on");
}
